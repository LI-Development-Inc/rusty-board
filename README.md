# 🦀 Rusty-Board

**Rusty-Board** is a high-performance, modular imageboard framework built in Rust. It is designed to scale from a single-user private archive to a global-scale community hub by utilizing a "compiled-to-order" architecture.

## 🚀 Key Features

- **Stateless Core**: Business logic is decoupled from infrastructure via Hexagonal Architecture.
- **Compiled-to-Order**: Use Cargo features to include only the drivers you need (e.g., SQLite vs. Postgres).
- **Extreme Efficiency**: Targeted binary size of < 5MB and RAM usage < 150MB.
- **Privacy First**: Ephemeral user IDs, hashed IPs, and no tracking by default.
- **Modern Media**: Native support for WebP, AVIF, and JPEG-XL via `libvips`.

## 🏗 Architecture

Rusty-Board uses a **Workspace-based Plugin System**. Instead of loading unstable dynamic libraries at runtime, features are enabled at compile-time for maximum safety and performance.

## 🛠 Feature Matrix

| Feature | Lite Build (Default) | Enterprise Build |
| :--- | :--- | :--- |
| **Database** | SQLite | PostgreSQL / Citus |
| **Storage** | Local Filesystem | S3 / R2 / Cloud Storage |
| **Auth** | Secure Tripcodes | OIDC / OAuth2 |
| **Search** | None | Meilisearch / FTS5 |

## 🏁 Quick Start (Development)

```bash
# Clone the repo
git clone [https://github.com/proton/rusty-board.git](https://github.com/proton/rusty-board.git)
cd rusty-board

# Run with default SQLite + Local Storage
cargo run -p rusty-board
