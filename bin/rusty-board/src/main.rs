// rusty-board/bin/rusty-board/src/main.rs
// Main entry point for Rusty-Board

use actix_web::{web, App, HttpServer};
use actix_files::Files;
use std::sync::Arc;
use rb_api::handlers::AppState;

// 1. Feature-gated imports
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

    // 2. Initialize Implementations with proper scoping
    // Note: We use Box::new because AppState expects Box<dyn Trait>
    
    #[cfg(feature = "db-sqlite")]
    let repo = Box::new(SqliteBoardRepo::new("sqlite:rusty_board.db").await
        .expect("Failed to init SQLite"));

    #[cfg(feature = "storage-local")]
    let store = Box::new(LocalMediaStore::new(
        "./data/uploads".into(), 
        "/static/uploads".into()
    ));

    #[cfg(feature = "auth-simple")]
    let auth = Box::new(SimpleAuthProvider::new("your-secret-salt"));

    // 3. Wrap in AppState
    // We use Arc to make the AppState sharable across Actix threads
    let state = web::Data::new(AppState {
        repo,
        store,
        auth,
    });

    log::info!("🚀 Rusty-Board starting on http://127.0.0.1:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(Files::new("/static/uploads", "./data/uploads").show_files_listing())
            .service(Files::new("/static", "static").show_files_listing())
            // Register your routes here
            .service(
                web::scope("")
                    .route("/", web::get().to(rb_api::handlers::index))
                    .route("/{board}/", web::get().to(rb_api::handlers::board_index)) 
                    .route("/{board}/thread/{id}", web::get().to(rb_api::handlers::view_thread))
                    .route("/{board}/post", web::post().to(rb_api::handlers::create_post))
                    .route("/{board}/catalog", web::get().to(rb_api::handlers::get_catalog))

                    // Add your other routes once handlers are ready
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}