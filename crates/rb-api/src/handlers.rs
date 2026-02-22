//! rusty-board/crates/rb-ui/src/handlers.rs Handlers
//! 
//! This module coordinates the flow between HTTP requests and Core traits.

use actix_web::{HttpRequest, HttpResponse, Responder, web};
use actix_multipart::Multipart;
use futures_util::stream::TryStreamExt;
use rb_core::models::{Post, Thread};
use rb_core::traits::{BoardRepo, MediaStore, AuthProvider};
use rb_ui::{IndexTemplate, ThreadTemplate, CatalogTemplate};
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
    mut payload: Multipart, 
) -> impl Responder {
    let client_ip = req.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
    
    let mut content = String::new();
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut content_type = String::new();
    let mut thread_id_from_form: Option<Uuid> = None;

    // 1. Process Multipart Stream
    while let Ok(Some(mut field)) = payload.try_next().await {
        let name = field.name().to_string();

        match name.as_str() {
            "content" => {
                while let Ok(Some(chunk)) = field.try_next().await {
                    content.push_str(std::str::from_utf8(&chunk).unwrap_or_default());
                }
            },
            "thread_id" => {
                while let Ok(Some(chunk)) = field.try_next().await {
                    let id_str = std::str::from_utf8(&chunk).unwrap_or_default();
                    if let Ok(id) = Uuid::parse_str(id_str) {
                        thread_id_from_form = Some(id);
                    }
                }
            },
            "file" => {
                content_type = field.content_type().map(|m| m.to_string()).unwrap_or_default();
                let mut bytes = Vec::new();
                while let Ok(Some(chunk)) = field.try_next().await {
                    bytes.extend_from_slice(&chunk);
                }
                if !bytes.is_empty() {
                    image_bytes = Some(bytes);
                }
            },
            _ => {}
        }
    }

    // 2. Security Check: Is the IP banned?
    if let Ok(true) = data.auth.check_ban(&client_ip).await {
        return HttpResponse::Forbidden().body("You are banned.");
    }

    // 3. Media: Process image if present
    let media_id = if let Some(bytes) = image_bytes {
        match data.store.save_upload(bytes, &content_type).await {
            Ok(id) => Some(id),
            Err(e) => {
                log::error!("Media storage error: {:?}", e);
                return HttpResponse::InternalServerError().body("Failed to save media");
            }
        }
    } else {
        None
    };

    // 4. Identify Context
    let board_slug = req.match_info().get("board").unwrap_or("b");
    let board = match data.repo.get_board(board_slug).await {
        Ok(Some(b)) => b,
        _ => return HttpResponse::NotFound().finish(),
    };

    let is_new_thread = thread_id_from_form.is_none();
    let thread_target = thread_id_from_form.unwrap_or_else(Uuid::now_v7);
    let user_id = data.auth.generate_thread_id(&client_ip, &thread_target.to_string());

    // 5. Create Post Model
    let new_post = Post {
        id: Uuid::now_v7(),
        thread_id: thread_target,
        user_id_in_thread: user_id,
        content: sanitize_content(&content),
        media_id,
        is_op: is_new_thread,
        created_at: Utc::now(),
        metadata: serde_json::json!({}),
    };

    // 6. Persistence Logic
    if is_new_thread {
        let new_thread = Thread {
            id: thread_target,
            board_id: board.id,
            last_bump: Utc::now(),
            is_sticky: false,
            is_locked: false,
            metadata: serde_json::json!({}),
        };
        if let Err(e) = data.repo.create_thread(new_thread, new_post).await {
            log::error!("DB Error (Thread): {:?}", e);
            return HttpResponse::InternalServerError().finish();
        }
    } else {
        if let Err(e) = data.repo.create_post(new_post).await {
            log::error!("DB Error (Post): {:?}", e);
            return HttpResponse::InternalServerError().finish();
        }
    }

    // Redirect to the thread view
    HttpResponse::SeeOther()
        .insert_header(("Location", format!("/{}/thread/{}", board_slug, thread_target)))
        .finish()
}

/// Renders the Board Index (e.g., /b/)
pub async fn board_index(
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let board_slug = path.into_inner();
    
    match data.repo.get_board(&board_slug).await {
        Ok(Some(board)) => {
            let threads = data.repo.get_threads_by_board(board.id).await.unwrap_or_default();
            
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

// Renders the Catalog view for a Board (e.g., /b/catalog)
pub async fn get_catalog(
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let slug = path.into_inner();
    let board = data.repo.get_board(&slug).await.unwrap().unwrap();
    let threads = data.repo.get_threads_by_board(board.id).await.unwrap();

    let s = CatalogTemplate {
        board: &board,
        threads: &threads,
        title: format!("/{}/ - Catalog", slug),
    }
    .render()
    .unwrap();

    HttpResponse::Ok().content_type("text/html").body(s)
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
            let html = ThreadTemplate {
                board: &board,
                thread: &thread,
                posts: &posts,
                title: format!("Thread #{} - / {} /", thread.id, board.slug),
                media_url: "/static/uploads/".to_string(),
                thumb_url: "/static/uploads/thumbs/".to_string(),
            }
            .render()
            .expect("Template rendering failed");

            HttpResponse::Ok().content_type("text/html").body(html)
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

pub async fn index(_data: web::Data<AppState>) -> impl Responder {
    HttpResponse::Ok().body("Welcome to Rusty-Board! Try going to /b/")
}

fn sanitize_content(raw: &str) -> String {
    let escaped = html_escape::encode_safe(raw).to_string();
    
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