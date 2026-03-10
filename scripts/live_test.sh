#!/usr/bin/env bash
# live_test.sh — smoke-test every rusty-board endpoint against a running server.
#
# Usage:  bash scripts/live_test.sh [BASE_URL]
# Default BASE_URL: http://localhost:8080
#
# Prerequisites:
#   - curl, python3
#   - Server running:  make watch
#   - DB seeded:       make db-reset && make migrate && make seed
#
# Exit code: 0 if all tests pass, non-zero on any failure.

set -euo pipefail
BASE="${1:-http://localhost:8080}"
PASS=0; FAIL=0; SKIP=0

GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[0;33m'; NC='\033[0m'
ok()   { echo -e "  ${GREEN}PASS${NC}  $1"; PASS=$((PASS+1)); }
fail() { echo -e "  ${RED}FAIL${NC}  $1 — $2"; FAIL=$((FAIL+1)); }
skip() { echo -e "  ${YELLOW}SKIP${NC}  $1 — $2"; SKIP=$((SKIP+1)); }
h1()   { echo; echo "── $1 ──────────────────────────────────────────────"; }

# Follow redirects; expect final status
get() {
  local path="$1" expect="$2" desc="${3:-GET $1}"
  local s; s=$(curl -s -L -o /dev/null -w "%{http_code}" "$BASE$path")
  [ "$s" = "$expect" ] && ok "$desc ($s)" || fail "$desc" "expected $expect got $s"
}

# No redirect following
get_nr() {
  local path="$1" expect="$2" desc="${3:-GET $1}"
  local s; s=$(curl -s -o /dev/null -w "%{http_code}" "$BASE$path")
  [ "$s" = "$expect" ] && ok "$desc ($s)" || fail "$desc" "expected $expect got $s"
}

# Follow redirects, check body contains pattern
get_html() {
  local path="$1" expect="$2" pattern="$3" desc="${4:-GET $1}"
  local tmp s
  tmp=$(mktemp)
  s=$(curl -s -L -o "$tmp" -w "%{http_code}" "$BASE$path")
  if [ "$s" != "$expect" ]; then fail "$desc" "HTTP $s"; rm -f "$tmp"; return; fi
  grep -q "$pattern" "$tmp" && ok "$desc" || fail "$desc" "missing '$pattern'"
  rm -f "$tmp"
}

# POST JSON; check status only
post_json() {
  local path="$1" body="$2" expect="$3" desc="${4:-POST $1}"
  local s; s=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST -H "Content-Type: application/json" -d "$body" "$BASE$path")
  [ "$s" = "$expect" ] && ok "$desc ($s)" || fail "$desc" "expected $expect got $s"
}

# Login; returns ONLY the token to stdout, nothing else.
login_token() {
  local user="$1" pass="$2"
  local tmp s tok
  tmp=$(mktemp)
  s=$(curl -s -o "$tmp" -w "%{http_code}" \
    -X POST -H "Content-Type: application/json" \
    -d "{\"username\":\"$user\",\"password\":\"$pass\"}" \
    "$BASE/auth/login")
  tok=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d.get('token',''))" "$tmp" 2>/dev/null || true)
  rm -f "$tmp"
  if [ "$s" = "200" ] && [ -n "$tok" ]; then echo "$tok"; return 0; fi
  return 1
}

authed_get() {
  local tok="$1" path="$2" expect="$3" desc="${4:-GET $path (bearer)}"
  local s; s=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $tok" "$BASE$path")
  [ "$s" = "$expect" ] && ok "$desc ($s)" || fail "$desc" "expected $expect got $s"
}

# Follow redirects then check final status
authed_get_follow() {
  local tok="$1" path="$2" expect="$3" desc="${4:-GET $path (bearer, follow)}"
  local s; s=$(curl -s -L -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $tok" "$BASE$path")
  [ "$s" = "$expect" ] && ok "$desc ($s)" || fail "$desc" "expected $expect got $s"
}

cookie_get() {
  local tok="$1" path="$2" expect="$3" desc="${4:-GET $path (cookie)}"
  local s; s=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Cookie: token=$tok" "$BASE$path")
  [ "$s" = "$expect" ] && ok "$desc ($s)" || fail "$desc" "expected $expect got $s"
}

echo "rusty-board live endpoint test"
echo "Target: $BASE"
echo "$(date)"

# ── 1. Infrastructure ─────────────────────────────────────────────────────────
h1 "Health & Static"
get    "/healthz"              200 "Health check"
get    "/static/css/style.css" 200 "Static CSS"

# ── 2. Public HTML pages ───────────────────────────────────────────────────────
h1 "Public HTML Pages"
get      "/"          200 "Root → overboard (redirect followed)"
get_nr   "/"          303 "Root issues 303"
get_html "/overboard"  200 "Overboard" "Overboard"
get_html "/auth/login" 200 "Login"     "Login"

# ── 3. Board API ──────────────────────────────────────────────────────────────
h1 "Board API"
get "/boards" 200 "GET /boards"

# /boards returns PageResponse: {"items":[...],"total":N,...} — not a plain array
SLUG=$(curl -s "$BASE/boards" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['items'][0]['slug'] if d.get('items') else '')" 2>/dev/null || true)
if [ -z "$SLUG" ]; then
  echo -e "  ${YELLOW}WARN${NC}  No boards in DB — run: make db-reset && make migrate && make seed"
  SLUG="b"  # fallback: try /b/ anyway
fi
echo "  Board: /$SLUG/"

get      "/boards/$SLUG"           200 "GET /boards/:slug (JSON)"
get_html "/board/$SLUG"            200 "$SLUG"   "Board index HTML"
get_html "/board/$SLUG/catalog"    200 "Catalog" "Catalog HTML"

# Trailing slash: fallback handler issues 308 redirect, curl -L follows to 200
get      "/board/$SLUG/"          200 "Trailing slash normalised (308→200)"
get      "/board/no_such_board_xyz"        404 "Unknown board → 404"

# ── 4. Thread HTML ─────────────────────────────────────────────────────────────
h1 "Thread HTML"
THREAD_ID=$(curl -s "$BASE/board/$SLUG" \
  | grep -oP '/board/[^/]+/thread/\K[0-9a-f-]{36}' | head -1 || true)
if [ -n "$THREAD_ID" ]; then
  get_html "/board/$SLUG/thread/$THREAD_ID"   200 "Reply to Thread" "Thread page"
  get      "/board/$SLUG/thread/$THREAD_ID/" 200 "Thread trailing slash normalised (308→200)"
else
  skip "Thread view" "no threads on board (run make seed)"
fi

# ── 5. Authentication ──────────────────────────────────────────────────────────
h1 "Authentication"
post_json "/auth/login" '{"username":"admin","password":"bad"}'    "401" "Bad password → 401"
post_json "/auth/login" '{"username":"nobody","password":"x"}'     "401" "Unknown user → 401"

TOKEN=$(login_token "admin" "admin123" 2>/dev/null || true)
if [ -n "$TOKEN" ]; then
  ok "Login admin (200)"
  echo "  Token: ${TOKEN:0:20}..."
  authed_get        "$TOKEN" "/admin/dashboard"   200 "Admin dashboard (bearer)"
  authed_get_follow "$TOKEN" "/mod/dashboard"    200 "Mod dashboard as admin → redirect → 200 (bearer)"
  cookie_get        "$TOKEN" "/admin/dashboard"  200 "Admin dashboard (cookie)"
  cookie_get        "$TOKEN" "/janitor/dashboard" 200 "Janitor dashboard as admin (cookie)"
  get_nr "/auth/logout" 303 "Logout → 303"
else
  fail "Login admin" "got 401 — DB may have stale users. Run: make db-reset && make migrate && make seed"
  skip "Authenticated endpoints" "no admin token"
fi

JANITOR_TOKEN=$(login_token "janitor" "janitor123" 2>/dev/null || true)
if [ -n "$JANITOR_TOKEN" ]; then
  ok "Login janitor (200)"
  cookie_get        "$JANITOR_TOKEN" "/janitor/dashboard" 200 "Janitor dashboard (cookie)"
  authed_get_follow "$JANITOR_TOKEN" "/mod/dashboard"    200 "/mod/dashboard redirect for janitor (follow)"
  cookie_get        "$JANITOR_TOKEN" "/admin/dashboard"  403 "Admin dashboard forbidden for janitor"
else
  fail "Login janitor" "got 401 — run: make db-reset && make migrate && make seed"
  skip "Janitor tests" "no janitor token"
fi

OWNER_TOKEN=$(login_token "board_owner" "owner123" 2>/dev/null || true)
if [ -n "$OWNER_TOKEN" ]; then
  ok "Login board_owner (200)"
  authed_get_follow "$OWNER_TOKEN" "/mod/dashboard" 200 "/mod/dashboard redirect for board_owner (follow)"
  cookie_get        "$OWNER_TOKEN"  "/admin/dashboard" 403 "Admin dashboard forbidden for board_owner"
else
  fail "Login board_owner" "got 401 — run: make db-reset && make migrate && make seed"
  skip "Board owner tests" "no board_owner token"
fi

# ── 6. Admin Board Management ──────────────────────────────────────────────────
h1 "Admin Board Management"
if [ -n "${TOKEN:-}" ]; then
  TMP=$(mktemp)
  CS=$(curl -s -o "$TMP" -w "%{http_code}" \
    -X POST -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{"slug":"livetest","title":"Live Test","rules":""}' \
    "$BASE/admin/boards")
  if [ "$CS" = "201" ]; then
    ok "POST /admin/boards (201)"
    CID=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1])).get('id',''))" "$TMP" 2>/dev/null || true)
    if [ -n "$CID" ]; then
      UPD=$(curl -s -o /dev/null -w "%{http_code}" \
        -X PUT -H "Content-Type: application/json" \
        -H "Authorization: Bearer $TOKEN" \
        -d '{"title":"Updated","rules":"No spam"}' \
        "$BASE/admin/boards/$CID")
      [ "$UPD" = "200" ] && ok "PUT /admin/boards/:id (200)" || fail "PUT board" "HTTP $UPD"
      DEL=$(curl -s -o /dev/null -w "%{http_code}" \
        -X DELETE -H "Authorization: Bearer $TOKEN" "$BASE/admin/boards/$CID")
      [ "$DEL" = "204" ] && ok "DELETE /admin/boards/:id (204)" || fail "DELETE board" "HTTP $DEL"
    fi
  elif [ "$CS" = "409" ]; then
    ok "POST /admin/boards (409 — exists from previous run)"
  else
    fail "POST /admin/boards" "HTTP $CS"
  fi
  rm -f "$TMP"
else
  skip "Admin board management" "no token"
fi

# ── 7. Moderation ──────────────────────────────────────────────────────────────
h1 "Moderation"
if [ -n "${TOKEN:-}" ]; then
  authed_get "$TOKEN" "/mod/flags" 200 "GET /mod/flags"
  authed_get "$TOKEN" "/mod/bans"  200 "GET /mod/bans"
  if [ -n "${THREAD_ID:-}" ]; then
    FS=$(curl -s -o /dev/null -w "%{http_code}" \
      -X POST -H "Content-Type: application/json" \
      -d '{"reason":"live-test report — ignore"}' \
      "$BASE/board/$SLUG/thread/$THREAD_ID/flag")
    [ "$FS" = "201" ] && ok "POST flag (201)" || fail "POST flag" "HTTP $FS"
  else
    skip "POST flag" "no thread ID"
  fi
else
  skip "Moderation" "no token"
fi

# ── 8. Post creation ──────────────────────────────────────────────────────────
h1 "Post Creation"
if [ -n "${THREAD_ID:-}" ]; then
  # Use a timestamp in the body to avoid duplicate-content detection on re-runs.
  # Use staff auth token (if available) to bypass rate limiting.
  UNIQUE_BODY="live-test reply $(date +%s)"
  POST_ARGS=(-X POST -F "thread_id=$THREAD_ID" -F "body=$UNIQUE_BODY")
  [ -n "${TOKEN:-}" ] && POST_ARGS+=(-H "Authorization: Bearer $TOKEN")
  PS=$(curl -s -o /dev/null -w "%{http_code}" "${POST_ARGS[@]}" "$BASE/board/$SLUG/post")
  if [ "$PS" = "303" ] || [ "$PS" = "302" ]; then
    ok "Text reply → redirect ($PS)"
  else
    fail "Text reply" "expected 302/303 redirect, got $PS"
  fi
else
  skip "Post creation" "no thread ID"
fi

# ── 9. Error handling ─────────────────────────────────────────────────────────
h1 "Error Handling"
get    "/board/no_such_board_xyz"  404 "Unknown board → 404"
get    "/board/$SLUG/thread/00000000-0000-0000-0000-000000000000" 404 "Unknown thread → 404"
get_nr "/admin/dashboard"          401 "Admin without auth → 401"
get_nr "/mod/dashboard"            401 "Mod dashboard without auth → 401"
get_nr "/janitor/dashboard"        401 "Janitor dashboard without auth → 401"
get_nr "/volunteer/dashboard"      401 "Volunteer dashboard without auth → 401"

# ── Summary ───────────────────────────────────────────────────────────────────
echo
echo "─────────────────────────────────────────────"
TOTAL=$((PASS+FAIL+SKIP))
echo -e "Results: ${GREEN}$PASS passed${NC}, ${RED}$FAIL failed${NC}, ${YELLOW}$SKIP skipped${NC} / $TOTAL total"
[ "$FAIL" -gt 0 ] && echo -e "${YELLOW}Tip: if login fails, run: make db-reset && make migrate && make seed${NC}
         New roles: admin/admin123, janitor/janitor123, board_owner/owner123, volunteer/vol123"
echo "─────────────────────────────────────────────"
[ "$FAIL" -eq 0 ]
