#!/usr/bin/env bash
# scripts/migrate.sh — Run SQLx database migrations against a live database.
#
# Migrations are embedded into the binary and run automatically at startup.
# Use this script when you want to run migrations BEFORE starting the application
# (e.g. in CI, during staging deploys, or before a blue-green switch).
#
# Requires: sqlx-cli (`cargo install sqlx-cli --no-default-features --features postgres`)
#
# Usage:
#   ./scripts/migrate.sh              # run all pending migrations
#   ./scripts/migrate.sh revert       # revert the most recent migration
#   ./scripts/migrate.sh info         # list migrations and their status
#
# Environment variables:
#   DB_URL — PostgreSQL connection string (required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
MIGRATIONS_DIR="$ROOT_DIR/crates/storage-adapters/src/migrations"

# Load .env if present
if [[ -f "$ROOT_DIR/.env" ]]; then
  # shellcheck disable=SC1090
  source <(grep -v '^#' "$ROOT_DIR/.env" | grep -v '^$')
fi

DB_URL="${DB_URL:-}"

if [[ -z "$DB_URL" ]]; then
  echo "ERROR: DB_URL is not set. Export it or add it to .env." >&2
  exit 1
fi

if ! command -v sqlx &>/dev/null; then
  echo "ERROR: sqlx-cli not found."
  echo "Install with: cargo install sqlx-cli --no-default-features --features postgres"
  exit 1
fi

COMMAND="${1:-run}"

case "$COMMAND" in
  run)
    echo "Running pending migrations against: $DB_URL"
    sqlx migrate run \
      --database-url "$DB_URL" \
      --source "$MIGRATIONS_DIR"
    echo "Migrations complete."
    ;;
  revert)
    echo "Reverting most recent migration..."
    sqlx migrate revert \
      --database-url "$DB_URL" \
      --source "$MIGRATIONS_DIR"
    echo "Revert complete."
    ;;
  info)
    sqlx migrate info \
      --database-url "$DB_URL" \
      --source "$MIGRATIONS_DIR"
    ;;
  *)
    echo "Usage: $0 [run | revert | info]"
    exit 1
    ;;
esac
