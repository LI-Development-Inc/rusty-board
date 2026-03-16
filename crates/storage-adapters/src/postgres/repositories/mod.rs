//! PostgreSQL implementations of all domain repository ports.

pub mod archive_repository;
pub mod audit_repository;
pub mod ban_repository;
pub mod board_repository;
pub mod flag_repository;
pub mod post_repository;
pub mod session_repository;
pub mod staff_message_repository;
pub mod staff_request_repository;
pub mod thread_repository;
pub mod user_repository;

pub use audit_repository::PgAuditRepository;
pub use ban_repository::PgBanRepository;
pub use board_repository::PgBoardRepository;
pub use flag_repository::PgFlagRepository;
pub use post_repository::PgPostRepository;
pub use session_repository::PgSessionRepository;
pub use staff_message_repository::PgStaffMessageRepository;
pub use staff_request_repository::PgStaffRequestRepository;
pub use thread_repository::PgThreadRepository;
pub use user_repository::PgUserRepository;
