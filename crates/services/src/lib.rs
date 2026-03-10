//! `services` — all business logic for rusty-board.
//!
//! Services are generic over port traits and contain zero references to concrete
//! adapter types. All conditional logic is driven by `BoardConfig` fields passed
//! as parameters — never by feature flags, env vars, or global state.
//!
//! # Module structure
//! - `board/` — board CRUD, slug validation, config management
//! - `thread/` — thread creation, sticky/close, prune trigger
//! - `post/` — ban check, rate limit, spam heuristics, media dispatch, insert
//! - `moderation/` — ban, flag, delete, audit log
//! - `user/` — create user, login, deactivate, register
//! - `staff_request/` — submit, list, approve, deny escalation requests
//! - `common/` — shared utilities (slug, pagination, ip_hash, spam scoring)

pub mod board;
pub mod common;
pub mod moderation;
pub mod post;
pub mod staff_message;
pub mod staff_request;
pub mod thread;
pub mod user;
