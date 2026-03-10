#!/usr/bin/env bash
# scripts/deploy.sh — Deploy rusty-board to a remote host or registry.
#
# Usage:
#   ./scripts/deploy.sh [--tag TAG] [--push] [--remote HOST]
#
# Environment variables (override via .env or export before running):
#   DOCKER_REGISTRY   — Container registry (default: ghcr.io/your-org/rusty-board)
#   DEPLOY_HOST       — SSH host for remote deployment (optional)
#   DEPLOY_USER       — SSH user for remote deployment (default: deploy)
#   APP_ENV           — Target environment: staging | production (default: staging)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# ── Defaults ──────────────────────────────────────────────────────────────────

DOCKER_REGISTRY="${DOCKER_REGISTRY:-ghcr.io/your-org/rusty-board}"
DEPLOY_USER="${DEPLOY_USER:-deploy}"
APP_ENV="${APP_ENV:-staging}"
TAG="${TAG:-latest}"
PUSH="${PUSH:-false}"
REMOTE="${REMOTE:-}"

# ── Parse arguments ───────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        --tag)      TAG="$2"; shift 2 ;;
        --push)     PUSH="true"; shift ;;
        --remote)   REMOTE="$2"; shift 2 ;;
        --env)      APP_ENV="$2"; shift 2 ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--tag TAG] [--push] [--remote HOST] [--env staging|production]"
            exit 1
            ;;
    esac
done

IMAGE="${DOCKER_REGISTRY}:${TAG}"
echo "==> Deploy: image=${IMAGE} env=${APP_ENV} push=${PUSH}"

# ── Build Docker image ────────────────────────────────────────────────────────

echo ""
echo "==> Building Docker image..."
cd "${PROJECT_ROOT}"
docker build \
    --tag "${IMAGE}" \
    --tag "${DOCKER_REGISTRY}:latest" \
    --build-arg "APP_ENV=${APP_ENV}" \
    .

echo "==> Built: ${IMAGE}"

# ── Push to registry ──────────────────────────────────────────────────────────

if [[ "${PUSH}" == "true" ]]; then
    echo ""
    echo "==> Pushing to registry..."
    docker push "${IMAGE}"
    docker push "${DOCKER_REGISTRY}:latest"
    echo "==> Pushed: ${IMAGE}"
fi

# ── Remote deployment via SSH ─────────────────────────────────────────────────

if [[ -n "${REMOTE}" ]]; then
    echo ""
    echo "==> Deploying to ${DEPLOY_USER}@${REMOTE}..."

    # Copy docker-compose.yml to remote if needed
    scp "${PROJECT_ROOT}/docker-compose.yml" \
        "${DEPLOY_USER}@${REMOTE}:/opt/rusty-board/docker-compose.yml"

    # Pull image and restart on remote
    ssh "${DEPLOY_USER}@${REMOTE}" bash -s << EOF
set -euo pipefail
cd /opt/rusty-board
echo "Pulling ${IMAGE}..."
docker pull "${IMAGE}"
echo "Restarting app..."
APP_IMAGE="${IMAGE}" docker compose up -d --no-build app
echo "Waiting for health check..."
sleep 5
docker compose ps
EOF

    echo "==> Deployed to ${REMOTE}"
fi

echo ""
echo "==> Deploy complete."
