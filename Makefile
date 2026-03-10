# rusty-board Makefile
# All commands assume you have Rust 1.75+, Docker, and Docker Compose installed.
# Run `make help` for a list of targets.

.DEFAULT_GOAL := help
.PHONY: help build test check fmt lint audit clean \
        db-up db-down db-reset migrate migrate-info migrate-add \
        redis-up infra-up infra-down \
        docker-build docker-run \
        sqlx-prepare cover watch \
        bench live-test seed

FEATURES ?= web-axum,db-postgres,auth-jwt,media-local,redis

# ─── Help ─────────────────────────────────────────────────────────────────────
help: ## Show this help
	@awk 'BEGIN{FS=":.*##"; printf "Usage:\n  make \033[36m<target>\033[0m\n\nTargets:\n"} \
	/^[a-zA-Z_-]+:.*##/ {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ─── Build ────────────────────────────────────────────────────────────────────
build: ## Build the release binary with default features
	cargo build --release --features "$(FEATURES)"

build-dev: ## Build the debug binary with default features
	cargo build --features "$(FEATURES)"

# ─── Test ─────────────────────────────────────────────────────────────────────
test: ## Run all unit tests
	cargo test --features "$(FEATURES)" -- --nocapture

test-integration: infra-up ## Run integration tests (requires running Postgres + Redis)
	cargo test --test '*' --features "$(FEATURES)" -- --nocapture

# ─── Code quality ─────────────────────────────────────────────────────────────
check: ## Run cargo check
	cargo check --all-targets --features "$(FEATURES)"

fmt: ## Auto-format all source code
	cargo fmt --all

fmt-check: ## Check formatting without modifying files
	cargo fmt --all -- --check

lint: ## Run clippy with deny(warnings)
	cargo clippy --all-targets --features "$(FEATURES)" -- -D warnings

audit: ## Run cargo-audit for security advisories
	cargo audit

# ─── Database ─────────────────────────────────────────────────────────────────
db-up: ## Start PostgreSQL in Docker
	docker compose up -d postgres

db-down: ## Stop PostgreSQL
	docker compose stop postgres

db-reset: ## Drop and recreate the database (destructive!)
	@echo "[db-reset] Terminating active connections to rusty_board..."
	docker compose exec postgres psql -U rusty -d postgres -c 	  "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = 'rusty_board' AND pid <> pg_backend_pid();" 	  > /dev/null 2>&1 || true
	@echo "[db-reset] Dropping and recreating rusty_board..."
	docker compose exec postgres psql -U rusty -d postgres -c "DROP DATABASE IF EXISTS rusty_board;"
	docker compose exec postgres psql -U rusty -d postgres -c "CREATE DATABASE rusty_board;"
	@echo "[db-reset] Done. Run: make migrate && make seed"
	$(MAKE) migrate

migrate: ## Run all pending SQL migrations (requires sqlx-cli and running Postgres)
	sqlx migrate run --database-url $${DB_URL:-postgresql://rusty:rusty@localhost:5432/rusty_board} \
	    --source crates/storage-adapters/src/migrations

migrate-info: ## Show migration status
	sqlx migrate info --database-url $${DB_URL:-postgresql://rusty:rusty@localhost:5432/rusty_board} \
	    --source crates/storage-adapters/src/migrations

migrate-add: ## Create a new migration file (usage: make migrate-add NAME=my_migration)
	sqlx migrate add --source crates/storage-adapters/src/migrations $(NAME)

seed: ## Seed boards, threads, posts, and all four staff roles (requires running server + migrated DB)
	@echo "[seed] Building seed binary..."
	@cargo build --bin seed -q
	@echo "[seed] Running seed script (requires make watch + make migrate first)..."
	@bash scripts/seed.sh

sqlx-prepare: ## Regenerate sqlx-data.json for offline mode (run after changing queries)
	SQLX_OFFLINE=false cargo sqlx prepare --workspace -- --features "$(FEATURES)"

# ─── Infrastructure ───────────────────────────────────────────────────────────
redis-up: ## Start Redis in Docker
	docker compose up -d redis

infra-up: ## Start all infrastructure services (Postgres + Redis + MinIO)
	docker compose up -d postgres redis minio minio_setup

infra-down: ## Stop all infrastructure services
	docker compose stop postgres redis minio

# ─── Docker ───────────────────────────────────────────────────────────────────
docker-build: ## Build the Docker image
	docker build --build-arg FEATURES="$(FEATURES)" -t rusty-board:latest .

docker-run: ## Run the Docker image (requires .env)
	docker compose up app

# ─── Development ──────────────────────────────────────────────────────────────
watch: ## Live-reload on source changes (requires cargo-watch)
	cargo watch -x "run --bin rusty-board --features $(FEATURES)"

cover: ## Run tests with code coverage (requires cargo-tarpaulin)
	cargo tarpaulin --features "$(FEATURES)" --out Html --output-dir target/coverage
	@echo "Coverage report: target/coverage/tarpaulin-report.html"

bench: ## Run criterion benchmarks
	cargo bench --features "$(FEATURES)"

# ─── Cleanup ──────────────────────────────────────────────────────────────────
clean: ## Remove build artifacts
	cargo clean

clean-all: clean ## Remove build artifacts and Docker volumes (destructive!)
	docker compose down -v

# ─── Live endpoint testing ─────────────────────────────────────────────────────
BASE_URL ?= http://localhost:8080

live-test: ## Smoke-test all endpoints against a running server (requires: make watch + make seed)
	@bash scripts/live_test.sh "$(BASE_URL)"
