//! # Rusty-Board Binary
//! 
//! The entry point that assembles the application based on compile-time features.

use actix_web::{web, App, HttpServer};
use rb_api::handlers::{handle_post, AppState};
use std::sync::Arc;

// Feature-gated imports: This is the "Compiled-to-Order" magic
#[cfg(feature = "db-sqlite")]
use rb_db_sqlite::SqliteBoardRepo;

#[cfg(feature = "storage-local")]
use rb_storage_local::LocalMediaStore;

#[cfg(feature = "auth-simple")]
use rb_auth_simple::SimpleAuthProvider;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // 1. Initialize Database Implementation
    #[cfg(feature = "db-sqlite")]
    let repo = SqliteBoardRepo::new("sqlite:rusty_board.db").await
        .expect("Failed to init SQLite");

    // 2. Initialize Storage Implementation
    #[cfg(feature = "storage-local")]
    let store = LocalMediaStore::new(
        "./data/uploads".into(), 
        "/static/uploads".into()
    );

    // 3. Initialize Auth Implementation
    #[cfg(feature = "auth-simple")]
    let auth = SimpleAuthProvider::new();

    // 4. Wrap in AppState (Using dynamic dispatch for maximum flexibility)
    let state = web::Data::new(AppState {
        repo: Box::new(repo),
        store: Box::new(store),
        auth: Box::new(auth),
    });

    log::info!("🚀 Rusty-Board starting on http://127.0.0.1:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(web::resource("/{board}/post").route(web::post().to(handle_post)))
            // TODO: Add index and thread view routes
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}