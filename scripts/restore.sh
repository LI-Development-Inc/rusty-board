#!/usr/bin/env bash
# scripts/restore.sh — Restore a PostgreSQL database and/or media files from backup.
#
# Usage:
#   ./scripts/restore.sh <backup-dir>
#   ./scripts/restore.sh <backup-dir> --db-only
#   ./scripts/restore.sh <backup-dir> --media-only
#
# <backup-dir> is the timestamped directory created by backup.sh, e.g.:
#   ./backups/20240115_143022
#
# Environment variables:
#   DB_URL       — PostgreSQL connection string (target database)
#   MEDIA_PATH   — Local media root directory (default: ./media)
#
# WARNING: Database restore DROPS AND RECREATES the target database.
# Confirm you want to overwrite the target before running.
#
# Exit codes:
#   0 — success
#   1 — usage error or missing dependency
#   2 — restore failure

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Load .env if present
if [[ -f "$ROOT_DIR/.env" ]]; then
  # shellcheck disable=SC1090
  source <(grep -v '^#' "$ROOT_DIR/.env" | grep -v '^$')
fi

DB_URL="${DB_URL:-}"
MEDIA_PATH="${MEDIA_PATH:-$ROOT_DIR/media}"

# ── Argument parsing ──────────────────────────────────────────────────────────
BACKUP_PATH=""
DO_DB=true
DO_MEDIA=true

for arg in "$@"; do
  case "$arg" in
    --db-only)    DO_MEDIA=false ;;
    --media-only) DO_DB=false ;;
    --help|-h)
      echo "Usage: $0 <backup-dir> [--db-only | --media-only]"
      exit 0
      ;;
    -*)
      echo "Unknown flag: $arg" >&2
      exit 1
      ;;
    *)
      if [[ -z "$BACKUP_PATH" ]]; then
        BACKUP_PATH="$arg"
      else
        echo "Unexpected argument: $arg" >&2
        exit 1
      fi
      ;;
  esac
done

if [[ -z "$BACKUP_PATH" ]]; then
  echo "ERROR: Backup directory is required." >&2
  echo "Usage: $0 <backup-dir> [--db-only | --media-only]" >&2
  exit 1
fi

if [[ ! -d "$BACKUP_PATH" ]]; then
  echo "ERROR: Backup directory not found: $BACKUP_PATH" >&2
  exit 1
fi

# ── Dependency checks ─────────────────────────────────────────────────────────
if $DO_DB; then
  if ! command -v pg_restore &>/dev/null; then
    echo "ERROR: pg_restore not found. Install postgresql-client." >&2
    exit 1
  fi
  if [[ -z "$DB_URL" ]]; then
    echo "ERROR: DB_URL is not set." >&2
    exit 1
  fi
fi

# ── Safety confirmation ───────────────────────────────────────────────────────
echo "==== RUSTY-BOARD RESTORE ===="
echo "Backup source: $BACKUP_PATH"
if $DO_DB; then
  echo "Target database: $DB_URL"
fi
if $DO_MEDIA; then
  echo "Target media:    $MEDIA_PATH"
fi
echo ""
echo "WARNING: This will OVERWRITE the current data."
read -rp "Type 'yes' to confirm: " confirmation
if [[ "$confirmation" != "yes" ]]; then
  echo "Restore cancelled."
  exit 0
fi

# ── Database restore ──────────────────────────────────────────────────────────
if $DO_DB; then
  DB_DUMP="$BACKUP_PATH/database.dump"
  if [[ ! -f "$DB_DUMP" ]]; then
    echo "ERROR: Database dump not found: $DB_DUMP" >&2
    exit 2
  fi

  # Extract DB name from URL for createdb/dropdb
  # Handles postgresql://user:pass@host:port/dbname
  DB_NAME="${DB_URL##*/}"
  DB_NAME="${DB_NAME%%\?*}"  # strip query string if present
  DB_BASE_URL="${DB_URL%/$DB_NAME}"

  echo "Dropping and recreating database '$DB_NAME'..."
  # Use postgres maintenance DB to drop/create
  MAINTENANCE_URL="${DB_BASE_URL}/postgres"
  psql "$MAINTENANCE_URL" -c "DROP DATABASE IF EXISTS $DB_NAME;" 2>/dev/null || true
  psql "$MAINTENANCE_URL" -c "CREATE DATABASE $DB_NAME;"

  echo "Restoring database from $DB_DUMP..."
  pg_restore \
    --no-password \
    --no-owner \
    --no-acl \
    --dbname="$DB_URL" \
    "$DB_DUMP"
  echo "Database restore complete."
fi

# ── Media restore ─────────────────────────────────────────────────────────────
if $DO_MEDIA; then
  MEDIA_ARCHIVE="$BACKUP_PATH/media.tar.gz"
  if [[ ! -f "$MEDIA_ARCHIVE" ]]; then
    echo "WARN: Media archive not found at $MEDIA_ARCHIVE — skipping media restore."
  else
    echo "Restoring media to $MEDIA_PATH..."
    # Backup existing media if present
    if [[ -d "$MEDIA_PATH" ]]; then
      MEDIA_BACKUP_TMP="$(mktemp -d)"
      mv "$MEDIA_PATH" "$MEDIA_BACKUP_TMP/"
      echo "  Existing media moved to: $MEDIA_BACKUP_TMP"
    fi

    mkdir -p "$(dirname "$MEDIA_PATH")"
    tar -xzf "$MEDIA_ARCHIVE" -C "$(dirname "$MEDIA_PATH")"
    echo "Media restore complete."
  fi
fi

echo ""
echo "Restore from $BACKUP_PATH complete."
