//! # rb-api Handlers
//! 
//! This module coordinates the flow between HTTP requests and Core traits.

use actix_web::{web, HttpResponse, HttpRequest};
use rb_core::models::{Post, Thread};
use rb_core::traits::{BoardRepo, MediaStore, AuthProvider};
use uuid::Uuid;
use chrono::Utc;

/// State shared across all Actix-web workers.
pub struct AppState {
    pub repo: Box<dyn BoardRepo>,
    pub store: Box<dyn MediaStore>,
    pub auth: Box<dyn AuthProvider>,
}

/// Orchestrates the creation of a new post or thread.
pub async fn handle_post(
    data: web::Data<AppState>,
    req: HttpRequest,
    form: web::Multipart, // Multi-part for file uploads
) -> HttpResponse {
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

pub async fn view_board_index(
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let board_slug = path.into_inner();
    
    // 1. Fetch board from repo
    match data.repo.get_board(&board_slug).await {
        Ok(Some(board)) => {
            // 2. Render index via rb-ui (Askama)
            // Placeholder: In real use, you'd call IndexTemplate { .. }.render()
            HttpResponse::Ok().body(format!("Welcome to / {} /", board.slug))
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

pub async fn view_thread(
    data: web::Data<AppState>,
    path: web::Path<(String, Uuid)>,
) -> HttpResponse {
    let (_board_slug, thread_id) = path.into_inner();

    match data.repo.get_thread(thread_id).await {
        Ok(Some((thread, posts))) => {
            // 3. Render thread via rb-ui (Askama)
            HttpResponse::Ok().body(format!("Thread {}: {} posts", thread.id, posts.len()))
        }
        _ => HttpResponse::NotFound().finish(),
    }
}