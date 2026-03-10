//! Seed binary — creates the initial dev users directly in the database.
//!
//! Uses the same `hash_password` function as the main app so logins work.
//!
//! Usage:
//!   cargo run --bin seed
//!   DB_URL=postgresql://... cargo run --bin seed

use anyhow::{Context, Result};
use auth_adapters::common::hashing;
use sqlx::postgres::PgPoolOptions;

const ARGON2_M: u32 = 19_456; // OWASP 2024 minimums — match app default
const ARGON2_T: u32 = 2;
const ARGON2_P: u32 = 1;

struct User {
    username: &'static str,
    password: &'static str,
    role:     &'static str,
}

const USERS: &[User] = &[
    User { username: "admin",       password: "admin123",   role: "admin"           },
    User { username: "janitor",     password: "janitor123", role: "janitor"         },
    User { username: "board_owner", password: "owner123",   role: "board_owner"     },
    User { username: "volunteer",   password: "vol123",     role: "board_volunteer" },
    User { username: "testuser",    password: "user123",    role: "user"            },
];

#[tokio::main]
async fn main() -> Result<()> {
    let db_url = std::env::var("DB_URL")
        .unwrap_or_else(|_| "postgresql://rusty:rusty@localhost:5432/rusty_board".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db_url)
        .await
        .context("failed to connect to database")?;

    println!("[seed] Hashing passwords and upserting users ...");

    for user in USERS {
        let hash = hashing::hash_password(user.password, ARGON2_M, ARGON2_T, ARGON2_P)
            .await
            .context(format!("failed to hash password for {}", user.username))?;

        sqlx::query(
            "INSERT INTO users (username, password_hash, role)
             VALUES ($1, $2, $3)
             ON CONFLICT (username) DO UPDATE
               SET password_hash = EXCLUDED.password_hash,
                   role          = EXCLUDED.role,
                   is_active     = true"
        )
        .bind(user.username)
        .bind(&hash.0)
        .bind(user.role)
        .execute(&pool)
        .await
        .context(format!("failed to upsert user {}", user.username))?;

        println!("[seed]   {} ({}) — OK", user.username, user.role);
    }

    println!("[seed] Done.");
    println!("[seed]   admin       / admin123     (admin)");
    println!("[seed]   janitor     / janitor123   (janitor)");
    println!("[seed]   board_owner / owner123     (board_owner)");
    println!("[seed]   volunteer   / vol123       (board_volunteer)");
    println!("[seed]   testuser    / user123      (user)");

    Ok(())
}
