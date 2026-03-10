#!/usr/bin/env bash
# scripts/backup.sh — Back up PostgreSQL database and local media files.
#
# Creates a timestamped backup directory containing:
#   - Full PostgreSQL dump (pg_dump, custom format)
#   - Tar archive of the local media directory
#
# Environment variables (read from .env if present):
#   DB_URL       — PostgreSQL connection string
#   MEDIA_PATH   — Local media root directory (default: ./media)
#   BACKUP_DIR   — Destination for backup files (default: ./backups)
#
# Usage:
#   ./scripts/backup.sh
#   ./scripts/backup.sh --media-only
#   ./scripts/backup.sh --db-only
#
# Exit codes:
#   0 — success
#   1 — missing dependency
#   2 — backup failure

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Load .env if present
if [[ -f "$ROOT_DIR/.env" ]]; then
  # shellcheck disable=SC1090
  source <(grep -v '^#' "$ROOT_DIR/.env" | grep -v '^$')
fi

DB_URL="${DB_URL:-}"
MEDIA_PATH="${MEDIA_PATH:-$ROOT_DIR/media}"
BACKUP_DIR="${BACKUP_DIR:-$ROOT_DIR/backups}"
TIMESTAMP="$(date -u +%Y%m%d_%H%M%S)"
BACKUP_PATH="$BACKUP_DIR/$TIMESTAMP"

DO_DB=true
DO_MEDIA=true

# ── Argument parsing ──────────────────────────────────────────────────────────
for arg in "$@"; do
  case "$arg" in
    --db-only)    DO_MEDIA=false ;;
    --media-only) DO_DB=false ;;
    --help|-h)
      echo "Usage: $0 [--db-only | --media-only]"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

# ── Dependency checks ─────────────────────────────────────────────────────────
if $DO_DB; then
  if ! command -v pg_dump &>/dev/null; then
    echo "ERROR: pg_dump not found. Install postgresql-client." >&2
    exit 1
  fi
  if [[ -z "$DB_URL" ]]; then
    echo "ERROR: DB_URL is not set. Export it or add it to .env." >&2
    exit 1
  fi
fi

# ── Setup ─────────────────────────────────────────────────────────────────────
mkdir -p "$BACKUP_PATH"
echo "Backup destination: $BACKUP_PATH"

# ── Database backup ───────────────────────────────────────────────────────────
if $DO_DB; then
  DB_DUMP="$BACKUP_PATH/database.dump"
  echo "Dumping database..."
  pg_dump \
    --format=custom \
    --no-password \
    "$DB_URL" \
    --file="$DB_DUMP"
  echo "Database dump written: $DB_DUMP ($(du -sh "$DB_DUMP" | cut -f1))"
fi

# ── Media backup ──────────────────────────────────────────────────────────────
if $DO_MEDIA; then
  MEDIA_ARCHIVE="$BACKUP_PATH/media.tar.gz"
  if [[ -d "$MEDIA_PATH" ]]; then
    echo "Archiving media from $MEDIA_PATH..."
    tar -czf "$MEDIA_ARCHIVE" -C "$(dirname "$MEDIA_PATH")" "$(basename "$MEDIA_PATH")"
    echo "Media archive written: $MEDIA_ARCHIVE ($(du -sh "$MEDIA_ARCHIVE" | cut -f1))"
  else
    echo "WARN: Media directory not found at $MEDIA_PATH — skipping media backup."
  fi
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "Backup complete: $BACKUP_PATH"
echo "Contents:"
ls -lh "$BACKUP_PATH"

# Remove backups older than 30 days (keeps disk from filling up)
find "$BACKUP_DIR" -maxdepth 1 -type d -mtime +30 -exec echo "Removing old backup: {}" \; -exec rm -rf {} +
