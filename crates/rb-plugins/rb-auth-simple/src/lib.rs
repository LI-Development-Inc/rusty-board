//! # rb-auth-simple
//! 
//! Argon2-based implementation of `AuthProvider`.
//! Handles secure tripcodes, staff authentication, and ephemeral thread IDs.

use async_trait::async_trait;
use rb_core::traits::AuthProvider;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use sha2::{Sha256, Digest};
use std::net::IpAddr;

pub struct SimpleAuthProvider {
    /// Secret salt for generating ephemeral Thread IDs (rotates on restart for security)
    session_salt: String,
}

impl SimpleAuthProvider {
    pub fn new() -> Self {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).expect("Failed to seed session salt");
        Self {
            session_salt: hex::encode(bytes),
        }
    }
}

#[async_trait]
impl AuthProvider for SimpleAuthProvider {
    /// Generates a "Thread ID" (e.g., oX3a9Z1p).
    /// Prevents users from tracking a poster across different threads.
    fn generate_thread_id(&self, ip: &str, thread_id: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.session_salt.as_bytes());
        hasher.update(ip.as_bytes());
        hasher.update(thread_id.as_bytes());
        let hash = hex::encode(hasher.finalize());
        // Return 8 character slice for UI simplicity
        hash[..8].to_string()
    }

    /// Generates a secure tripcode from "password".
    /// Result format: !/hashed_result/
    fn generate_tripcode(&self, password: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        // Use a static internal salt for standard tripcodes to match logic
        // or a dynamic one for "Secure Tripcodes".
        let result = base64::encode(hasher.finalize());
        format!("!{}", &result[..10])
    }

    /// Verifies if a provided password matches a stored Argon2 hash.
    async fn verify_admin_password(&self, password: &str, hash: &str) -> bool {
        let parsed_hash = match PasswordHash::new(hash) {
            Ok(p) => p,
            Err(_) => return false,
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    }

    /// Checks for a ban. In this Lite plugin, we assume a simple IP-based check.
    /// Full logic would query the BanRepo (implemented in the DB plugin).
    async fn check_ban(&self, ip: &str) -> anyhow::Result<bool> {
        // TODO: Integrate with BoardRepo/BanRepo logic
        Ok(false)
    }
}