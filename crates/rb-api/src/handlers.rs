//! # rb-api Handlers
//! 
//! This module coordinates the flow between HTTP requests and Core traits.

use actix_web::{HttpRequest, HttpResponse, Responder, web};
use actix_multipart::Multipart;
use rb_core::models::{Post, Thread};
use rb_core::traits::{BoardRepo, MediaStore, AuthProvider};
use rb_ui::{IndexTemplate, ThreadTemplate}; // Import your templates
use askama::Template;
use uuid::Uuid;
use chrono::Utc;

/// State shared across all Actix-web workers.
pub struct AppState {
    pub repo: Box<dyn BoardRepo>,
    pub store: Box<dyn MediaStore>,
    pub auth: Box<dyn AuthProvider>,
}

/// Orchestrates the creation of a new post or thread.
pub async fn create_post(
    data: web::Data<AppState>,
    req: HttpRequest,
    _form: Multipart, // Multi-part for file uploads
) -> impl Responder {
    let client_ip = req.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();

    // 1. Security Check: Is the IP banned?
    if let Ok(true) = data.auth.check_ban(&client_ip).await {
        return HttpResponse::Forbidden().body("You are banned.");
    }

    // 2. Logic: Process multipart form (simplified for brevity)
    // TODO: Implement a robust multipart parser to extract content and files
    let content = "User post content".to_string(); 
    let thread_id: Option<Uuid> = None; // None means new thread

    // 3. Media: Process image if present
    let media_id = if let Some(file_bytes) = Some(vec![]) { // Placeholder
        match data.store.save_upload(file_bytes, "image/jpeg").await {
            Ok(id) => Some(id),
            Err(_) => return HttpResponse::InternalServerError().finish(),
        }
    } else {
        None
    };

    // 4. Identity: Generate Tripcode and Thread ID
    let thread_target = thread_id.unwrap_or_else(Uuid::now_v7);
    let user_id = data.auth.generate_thread_id(&client_ip, &thread_target.to_string());

    // 5. Persistence: Save to DB
    let new_post = Post {
        id: Uuid::now_v7(),
        thread_id: thread_target,
        user_id_in_thread: user_id,
        content: sanitize_content(&content),
        media_id,
        is_op: thread_id.is_none(),
        created_at: Utc::now(),
        metadata: serde_json::json!({}),
    };

    if thread_id.is_none() {
        let new_thread = Thread {
            id: thread_target,
            board_id: Uuid::nil(), // Placeholder for board context
            last_bump: Utc::now(),
            is_sticky: false,
            is_locked: false,
            metadata: serde_json::json!({}),
        };
        data.repo.create_thread(new_thread, new_post).await.unwrap();
    } else {
        data.repo.create_post(new_post).await.unwrap();
    }

    HttpResponse::SeeOther()
        .insert_header(("Location", format!("/thread/{}", thread_target)))
        .finish()
}

/// Renders the Board Index (e.g., /b/)
pub async fn board_index(
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let board_slug = path.into_inner();
    
    // 1. Fetch board and its threads from repo
    match data.repo.get_board(&board_slug).await {
        Ok(Some(board)) => {
            // Get threads for this board (you might need a get_threads method in your trait)
            let threads = data.repo.get_threads_by_board(board.id).await.unwrap_or_default();
            
            // 2. Render via rb-ui Askama Template
            let html = IndexTemplate {
                board: &board,
                threads: &threads,
                title: format!("/ {} / - {}", board.slug, board.title),
            }
            .render()
            .expect("Template rendering failed");

            HttpResponse::Ok().content_type("text/html").body(html)
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

/// Renders a specific Thread (e.g., /b/thread/<uuid>)
pub async fn view_thread(
    data: web::Data<AppState>,
    path: web::Path<(String, Uuid)>,
) -> impl Responder {
    let (board_slug, thread_id) = path.into_inner();

    let board = match data.repo.get_board(&board_slug).await {
        Ok(Some(b)) => b,
        _ => return HttpResponse::NotFound().finish(),
    };

    match data.repo.get_thread(thread_id).await {
        Ok(Some((thread, posts))) => {
            // 3. Render via rb-ui Askama Template
            let html = ThreadTemplate {
                board: &board,
                thread: &thread,
                posts: &posts,
                title: format!("Thread #{} - / {} /", thread.id, board.slug),
                media_url: "/static/uploads/".to_string(), // Adjust as needed
                thumb_url: "/static/uploads/thumbs/".to_string(),
            }
            .render()
            .expect("Template rendering failed");

            HttpResponse::Ok().content_type("text/html").body(html)
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

/// Optional: A simple homepage handler for "/"
pub async fn index(_data: web::Data<AppState>) -> impl Responder {
    HttpResponse::Ok().body("Welcome to Rusty-Board! Try going to /b/")
}


/// Basic sanitization and "Greentext" transformation.
fn sanitize_content(raw: &str) -> String {
    // Escape HTML to prevent XSS
    let escaped = html_escape::encode_safe(raw).to_string();
    
    // Simple Greentext: lines starting with '>' become green
    escaped.lines()
        .map(|line| {
            if line.starts_with("&gt;") {
                format!("<span class=\"greentext\">{}</span>", line)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("<br />")
}
