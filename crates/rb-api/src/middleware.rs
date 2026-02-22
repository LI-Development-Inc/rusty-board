//! rusty-board/crates/rb-api/src/middleware.rs Middleware
//! 
//! Custom middleware for security, logging, and traffic control.

use actix_web::middleware::Logger;
// use actix_web::middleware::{Logger, NormalizePath, TrailingSlash};
use actix_cors::Cors;

// Returns a standard set of middleware for the Rusty-Board API.
pub fn standard_middleware() -> Logger {
    // We use the 'default' logger which outputs:
    // remote-ip "request-line" status-code response-size "referrer" "user-agent"
    Logger::default()
}

// Configures CORS (Cross-Origin Resource Sharing)
// Important if the UI and API ever live on different subdomains.
pub fn cors_policy() -> Cors {
    Cors::default()
        .allow_any_origin()
        .allowed_methods(vec!["GET", "POST"])
        .max_age(3600)
}

// Security Header Logic
// TODO: Implement a custom middleware to inject:
// - Content-Security-Policy (CSP)
// - X-Content-Type-Options: nosniff
// - Referrer-Policy: strict-origin-when-cross-origin