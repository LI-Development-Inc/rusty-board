//! CORS middleware.
//!
//! Single-origin deployments (operator behind a reverse proxy) do not require
//! CORS headers — the browser's same-origin policy handles everything.
//!
//! When federation (`web-activitypub`, v2.0) is enabled, a permissive
//! `Access-Control-Allow-Origin: *` on `/api/` routes will be needed for
//! ActivityPub clients. That implementation lives in the `federation-activitypub`
//! feature and will be added in v2.0.
//!
//! **Nothing to configure here for v1.x.** The module is declared so the
//! middleware stack can be extended without a module-tree restructure.
