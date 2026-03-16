//! Login brute-force protection.
//!
//! Tracks failed login attempts per username in a shared in-process map.
//! After `MAX_FAILURES` consecutive failures the account is locked out for
//! `LOCKOUT_SECS` seconds.  State resets on server restart — persistent
//! lockout would require a database-backed adapter and is planned for v1.2.
//!
//! Used as an `axum::Extension` injected in `composition.rs`.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_FAILURES: u32 = 5;
const LOCKOUT_SECS: u64 = 600; // 10 minutes

#[derive(Default, Clone)]
/// Shared in-process login attempt tracker.
///
/// Injected as an `axum::Extension` so every request can reach it without
/// adding a generic parameter to the auth handler state.
pub struct LoginGuard(Arc<Mutex<HashMap<String, Record>>>);

#[derive(Clone)]
struct Record {
    failures:     u32,
    locked_until: Option<Instant>,
}

impl LoginGuard {
    /// Create a new empty guard with no locked accounts.
    pub fn new() -> Self {
        LoginGuard(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Returns `Ok(())` if the username is not locked out,
    /// or `Err(seconds_remaining)` if it is.
    /// Check whether `username` is currently locked out.
    ///
    /// Returns `Ok(())` if posting is allowed, `Err(seconds_remaining)` if locked.
    pub fn check(&self, username: &str) -> Result<(), u64> {
        let map = self.0.lock().unwrap();
        if let Some(rec) = map.get(username) {
            if let Some(until) = rec.locked_until {
                if until > Instant::now() {
                    let secs = until.duration_since(Instant::now()).as_secs() + 1;
                    return Err(secs);
                }
            }
        }
        Ok(())
    }

    /// Record a failed login attempt. Returns the new failure count.
    /// Record a failed login attempt. Locks the account after `MAX_FAILURES` attempts.
    ///
    /// Returns the new consecutive failure count.
    pub fn record_failure(&self, username: &str) -> u32 {
        let mut map = self.0.lock().unwrap();
        let rec = map.entry(username.to_owned()).or_insert(Record {
            failures: 0, locked_until: None,
        });
        // Clear a past lockout before counting again.
        if rec.locked_until.map(|t| t <= Instant::now()).unwrap_or(false) {
            rec.failures = 0;
            rec.locked_until = None;
        }
        rec.failures += 1;
        if rec.failures >= MAX_FAILURES {
            rec.locked_until = Some(Instant::now() + Duration::from_secs(LOCKOUT_SECS));
        }
        rec.failures
    }

    /// Clear the failure record on a successful login.
    /// Clear the failure record on a successful login, unlocking the account.
    pub fn record_success(&self, username: &str) {
        self.0.lock().unwrap().remove(username);
    }
}
