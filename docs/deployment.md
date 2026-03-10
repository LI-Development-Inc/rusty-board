# deployment.md
# rusty-board — Deployment Guide

## docker-compose (Single Server)

The simplest production deployment runs everything on one machine via Docker Compose.

### Prerequisites

- Docker Engine 24+ and Docker Compose v2
- A domain name with DNS pointing to your server
- TLS termination via nginx/Caddy/Traefik (not handled by rusty-board)

### Steps

```bash
# 1. On the server, create the application directory
mkdir -p /opt/rusty-board && cd /opt/rusty-board

# 2. Copy docker-compose.yml and .env.example
scp docker-compose.yml .env.example user@yourserver:/opt/rusty-board/

# 3. Configure environment
cp .env.example .env
# Edit .env — set real secrets, domain, S3 endpoint, etc.

# 4. Start the stack
docker compose up -d

# 5. Verify health
curl http://localhost:8080/healthz

# 6. Run initial migration (first deploy only)
docker compose exec app sqlx migrate run
```

### Updating

```bash
# Pull the new image
docker compose pull app

# Restart with zero downtime (health check ensures new container is healthy before old is killed)
docker compose up -d --no-build app
```

---

## Environment Variables (Production)

Set these in `/opt/rusty-board/.env` on the server. Never commit real secrets.

| Variable | Required | Notes |
|----------|----------|-------|
| `APP_HOST` | No | Default `0.0.0.0` |
| `APP_PORT` | No | Default `8080` |
| `APP_ENV` | Yes | Set to `production` |
| `DATABASE_URL` | Yes | `postgres://user:pass@host/dbname` |
| `REDIS_URL` | Yes | `redis://:pass@host:6379` |
| `JWT_SECRET` | Yes | 32+ random chars — keep secret |
| `JWT_TTL_SECS` | No | Default 86400 (24h) |
| `S3_ENDPOINT` | Yes | MinIO URL or AWS endpoint |
| `S3_REGION` | Yes | `us-east-1` or MinIO region |
| `S3_ACCESS_KEY_ID` | Yes | S3/MinIO access key |
| `S3_SECRET_ACCESS_KEY` | Yes | S3/MinIO secret |
| `MEDIA_BUCKET` | Yes | S3 bucket name |
| `SHUTDOWN_TIMEOUT_SECS` | No | Default 30 — drain time on SIGTERM |

---

## TLS / Reverse Proxy

rusty-board does not terminate TLS. Put it behind nginx or Caddy.

### Caddy example (automatic HTTPS)

```caddy
rusty-board.example.com {
    reverse_proxy localhost:8080
}
```

### nginx example

```nginx
server {
    listen 443 ssl http2;
    server_name rusty-board.example.com;

    ssl_certificate     /etc/letsencrypt/live/rusty-board.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/rusty-board.example.com/privkey.pem;

    # Pass real client IP to app for IP hashing
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_set_header X-Real-IP       $remote_addr;

    location / {
        proxy_pass         http://127.0.0.1:8080;
        proxy_set_header   Host $host;
        client_max_body_size 10M;  # Match max_file_size_kb in BoardConfig
    }
}
```

---

## Backup and Restore

See `scripts/backup.sh` and `scripts/restore.sh`.

```bash
# Backup (run as a cron job — e.g. daily at 3 AM)
0 3 * * * /opt/rusty-board/scripts/backup.sh >> /var/log/rusty-board-backup.log 2>&1

# Restore from backup
./scripts/restore.sh /path/to/backup-2026-02-25.tar.gz
```

The backup script produces:
- A `pg_dump` of the database
- An S3 sync of the media bucket (or rsync for local filesystem storage)

---

## Health and Monitoring

### Health endpoint

```bash
curl https://rusty-board.example.com/healthz
# {"status":"ok","checks":{"postgres":"ok","redis":"ok"}}
```

Configure your load balancer or uptime monitor to check this endpoint.

### Prometheus metrics

`GET /metrics` returns Prometheus text format. Scrape with a Prometheus server and visualise with Grafana.

Key metrics:
- `http_requests_total` — request count by method and status
- `http_request_duration_seconds` — latency histogram
- `rate_limit_hits_total` — rate limiter rejections
- `spam_rejections_total` — spam filter rejections
- `thread_prunes_total` — threads pruned due to capacity

### Logging

All logs are structured JSON on stdout. Ship to a log aggregator (Loki, Elasticsearch, CloudWatch):

```bash
docker compose logs -f app | your-log-shipper
```

---

## Kubernetes (Helm Chart)

A Helm chart is available at `helm/rusty-board/`. Deploy with:

```bash
helm install rusty-board ./helm/rusty-board \
  --set image.tag=v1.0.0 \
  --set postgresql.enabled=true \
  --set redis.enabled=true \
  --values ./helm/values-production.yaml
```

The chart configures:
- Deployment with rolling update strategy
- PodDisruptionBudget (min 1 replica available)
- HorizontalPodAutoscaler (CPU-based)
- Ingress with TLS via cert-manager
- PostgreSQL and Redis as sub-charts (or external connection strings)
