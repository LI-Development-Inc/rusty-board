//! Moderation routes: flags, bans, delete, sticky, close.
//! TODO v1.2: add cycle and pin routes once handlers are implemented:
//!   POST /mod/threads/{id}/cycle  → toggle_cycle
//!   POST /mod/posts/{id}/pin      → set_post_pinned

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use services::board::BoardRepo;
use services::moderation::ModerationService;

use crate::axum::handlers::moderation_handlers::{self, ModerationDashboardState};

/// Moderation routes — require `Moderator` role or above.
pub fn moderation_routes<BR, PR, TR, FR, AR, UR, BS>(
    mod_service:   Arc<ModerationService<BR, PR, TR, FR, AR, UR>>,
    board_service: Arc<BS>,
) -> Router
where
    BR: domains::ports::BanRepository + 'static,
    PR: domains::ports::PostRepository + 'static,
    TR: domains::ports::ThreadRepository + 'static,
    FR: domains::ports::FlagRepository + 'static,
    AR: domains::ports::AuditRepository + 'static,
    UR: domains::ports::UserRepository + 'static,
    BS: BoardRepo + 'static,
{
    // Dashboard routes need both mod_svc and board_svc.
    let dashboard_state = ModerationDashboardState {
        mod_svc:   mod_service.clone(),
        board_svc: board_service,
    };

    Router::new()
        // ── Dashboards — role-specific ─────────────────────────────────────────
        .route("/mod/dashboard", get(moderation_handlers::mod_dashboard_redirect))
        .route(
            "/janitor/dashboard",
            get(moderation_handlers::janitor_dashboard::<BR, PR, TR, FR, AR, UR, BS>)
                .with_state(dashboard_state.clone()),
        )
        .route(
            "/volunteer/dashboard",
            get(moderation_handlers::volunteer_dashboard::<BR, PR, TR, FR, AR, UR, BS>)
                .with_state(dashboard_state),
        )
        // ── Public board-scoped routes (no auth required) ──────────────────────
        .route(
            "/board/{slug}/thread/{id}/flag",
            post(moderation_handlers::create_flag::<BR, PR, TR, FR, AR, UR>),
        )
        // ── Flags ──────────────────────────────────────────────────────────────
        .route("/mod/flags", get(moderation_handlers::list_flags::<BR, PR, TR, FR, AR, UR>))
        .route("/mod/flags/{id}/resolve", post(moderation_handlers::resolve_flag::<BR, PR, TR, FR, AR, UR>))
        // ── Post & thread management ───────────────────────────────────────────
        .route("/mod/posts/{id}/delete", post(moderation_handlers::delete_post::<BR, PR, TR, FR, AR, UR>))
        .route("/mod/threads/{id}/delete", post(moderation_handlers::delete_thread::<BR, PR, TR, FR, AR, UR>))
        .route("/mod/threads/{id}/delete-by-ip", post(moderation_handlers::delete_posts_by_ip::<BR, PR, TR, FR, AR, UR>))
        .route("/mod/threads/{id}/sticky", post(moderation_handlers::toggle_sticky::<BR, PR, TR, FR, AR, UR>))
        .route("/mod/threads/{id}/close", post(moderation_handlers::toggle_closed::<BR, PR, TR, FR, AR, UR>))
        // ── Bans ───────────────────────────────────────────────────────────────
        .route(
            "/mod/bans",
            get(moderation_handlers::list_bans::<BR, PR, TR, FR, AR, UR>)
                .post(moderation_handlers::create_ban::<BR, PR, TR, FR, AR, UR>),
        )
        .route("/mod/bans/{id}/expire", post(moderation_handlers::expire_ban::<BR, PR, TR, FR, AR, UR>))
        // ── Audit log pages ────────────────────────────────────────────────────
        .route(
            "/janitor/logs",
            get(moderation_handlers::janitor_audit_log::<BR, PR, TR, FR, AR, UR>),
        )
        .route(
            "/board-owner/logs",
            get(moderation_handlers::board_owner_audit_log::<BR, PR, TR, FR, AR, UR>),
        )
        .route(
            "/volunteer/logs",
            get(moderation_handlers::volunteer_audit_log::<BR, PR, TR, FR, AR, UR>),
        )
        .with_state(mod_service)
}
