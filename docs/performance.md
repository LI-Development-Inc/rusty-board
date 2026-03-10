# performance.md
# rusty-board — Performance Targets and Tuning

## v1.0 Targets

| Metric | Target | Notes |
|--------|--------|-------|
| `GET /board/:slug` (cached config) | < 10ms p99 | Board index, 1 DB query |
| `POST /board/:slug/post` (text only) | < 50ms p99 | Includes ban check, rate limit, DB write |
| `POST /board/:slug/post` (with image) | < 500ms p99 | Includes EXIF strip, thumbnail generation |
| Concurrent connections | 1000+ | Axum on Tokio, fully async |
| Throughput | 5,000 req/s | Single instance, cached hot paths |

These targets assume:
- PostgreSQL on localhost or LAN (< 2ms query latency)
- Redis on localhost (< 1ms)
- Image thumbnailing for JPEG up to 4MB

---

## Key Design Decisions for Performance

### BoardConfig caching

`BoardConfig` is loaded from PostgreSQL at most once per 60 seconds per board, then served from `BoardConfigCache` (in-memory `DashMap`). This eliminates a DB round-trip from every request on busy boards.

Cache TTL is configurable via `BOARD_CONFIG_CACHE_TTL_SECS` (default: 60).

### No N+1 queries

The `board_config_middleware` loads the board and its config in a single middleware call, injecting `ExtractedBoardConfig` into request extensions. Handlers access it from the extension — no additional DB call.

### Connection pooling

`sqlx::PgPool` is configured with:
- `max_connections`: 20 (default, increase for high traffic)
- `min_connections`: 2
- `connect_timeout`: 30s
- `idle_timeout`: 600s

Tune via `DATABASE_MAX_CONNECTIONS` env var.

### Async throughout

All I/O is `async` — no blocking calls on the Tokio executor. Argon2 hashing (login only) is offloaded to `tokio::task::spawn_blocking`.

### Media processing

Image thumbnailing (`image` crate + `oxipng`) is CPU-intensive. For high-volume boards, consider:
1. Running with multiple Tokio worker threads (`TOKIO_WORKER_THREADS`)
2. Offloading to a dedicated media processing service (future v1.x feature)

---

## Benchmarking

```bash
# Install wrk
sudo apt install wrk

# Benchmark board index
wrk -t4 -c100 -d30s http://localhost:8080/board/b

# Benchmark boards list
wrk -t4 -c100 -d30s http://localhost:8080/boards

# Health check throughput
wrk -t1 -c10 -d10s http://localhost:8080/healthz
```

---

## Tuning Checklist

For production deployments handling significant traffic:

- [ ] Set `DATABASE_MAX_CONNECTIONS` to `(CPU cores × 2)` or match Postgres `max_connections`
- [ ] Set `BOARD_CONFIG_CACHE_TTL_SECS` to 300 for stable boards (reduces DB load further)
- [ ] Enable gzip compression at the reverse proxy or rely on `CompressionLayer` (enabled)
- [ ] Use `media-s3` feature with a CDN in front of S3 for media serving
- [ ] Set `TOKIO_WORKER_THREADS` to number of physical CPU cores for image-heavy workloads
- [ ] Monitor `http_request_duration_seconds` histogram — p99 > targets indicates a bottleneck
- [ ] Monitor `rate_limit_hits_total` — sudden spike indicates a spam campaign
