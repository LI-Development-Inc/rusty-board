#!/usr/bin/env bash
# =============================================================================
# seed.sh — Populate rusty-board with realistic dev/test data
#
# Called from: make seed  (which pre-builds the seed binary first)
# Usage:       make seed
#              BASE_URL=http://... make seed
# =============================================================================

set -euo pipefail

DB_URL="${DB_URL:-postgresql://rusty:rusty@localhost:5432/rusty_board}"
BASE_URL="${BASE_URL:-http://localhost:8080}"
ADMIN_PASS="admin123"
JANITOR_PASS="janitor123"
VOLUNTEER_PASS="vol123"
OWNER_PASS="owner123"

GREEN='\033[0;32m'; RED='\033[0;31m'; NC='\033[0m'
info() { echo -e "${GREEN}[seed]${NC} $*"; }
die()  { echo -e "${RED}[error]${NC} $*" >&2; exit 1; }

command -v curl    >/dev/null || die "curl not found."
command -v python3 >/dev/null || die "python3 not found."

info "Checking server is up at $BASE_URL ..."
curl -sf "$BASE_URL/healthz" >/dev/null || die "Server not responding at $BASE_URL — is 'make watch' running?"

# ── 1. Create users via pre-built seed binary ─────────────────────────────────
# The binary was built by 'make seed' before calling this script.
# Running it directly avoids triggering cargo watch on the main app.
SEED_BIN="./target/debug/seed"
[[ -x "$SEED_BIN" ]] || die "Seed binary not found at $SEED_BIN — run 'make seed' not the script directly."

info "Creating users ..."
DB_URL="$DB_URL" "$SEED_BIN"

# ── 2. Login — retry up to 5 times with a short backoff ──────────────────────
# The main app may briefly restart if cargo watch detects workspace changes.
info "Logging in as admin ..."
TOKEN=""
for i in 1 2 3 4 5; do
    RESP=$(curl -s -o /tmp/seed_login_resp.json -w "%{http_code}" \
        -X POST "$BASE_URL/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"admin\",\"password\":\"$ADMIN_PASS\"}")

    if [[ "$RESP" == "200" ]]; then
        TOKEN=$(python3 -c "import json; print(json.load(open('/tmp/seed_login_resp.json'))['token'])")
        break
    fi

    if [[ $i -lt 5 ]]; then
        info "Login returned HTTP $RESP — server may be restarting, retrying in 2s ... ($i/5)"
        sleep 2
    else
        echo -e "${RED}[error]${NC} Login failed after 5 attempts. HTTP $RESP response:" >&2
        cat /tmp/seed_login_resp.json >&2
        echo "" >&2
        die "Check 'make watch' output for errors."
    fi
done

AUTH="Authorization: Bearer $TOKEN"
info "JWT obtained."

# ── Helpers ───────────────────────────────────────────────────────────────────
api_post() {
    curl -sf -X POST "$BASE_URL$1" -H "Content-Type: application/json" -H "$AUTH" -d "$2"
}
create_board() {
    api_post /admin/boards "{\"slug\":\"$1\",\"title\":\"$2\",\"rules\":\"$3\"}" \
        | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])"
}
BOUNDARY="RustyBoardSeed1234"
post_to_board() {
    # POST a multipart form to the board. On success the server issues a 303
    # redirect to /board/{slug}/thread/{uuid}#post-{n}. We capture the
    # Location header and print it so callers can extract the thread UUID.
    local slug="$1" text="$2" tid="${3:-}"
    local mp="--$BOUNDARY\r\nContent-Disposition: form-data; name=\"body\"\r\n\r\n${text}\r\n"
    [[ -n "$tid" ]] && mp+="--$BOUNDARY\r\nContent-Disposition: form-data; name=\"thread_id\"\r\n\r\n${tid}\r\n"
    mp+="--$BOUNDARY--\r\n"
    local hdr_file; hdr_file=$(mktemp)
    printf "%b" "$mp" | curl -s -X POST "$BASE_URL/board/$slug/post" \
        -H "Content-Type: multipart/form-data; boundary=$BOUNDARY" \
        -H "$AUTH" -D "$hdr_file" --data-binary @- > /dev/null
    # Print the Location header so the caller can pipe it through tid()
    grep -i '^location:' "$hdr_file" | tr -d '\r\n' | sed 's/[Ll]ocation: *//'
    rm -f "$hdr_file"
}
# Extract the thread UUID from the Location URL printed by post_to_board.
# Location format: /board/{slug}/thread/{uuid}#post-{n}
tid() { grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1; }

# ── 3. Boards ─────────────────────────────────────────────────────────────────
# Backfill any boards created without a board_configs row (pre-fix boards)
psql "$DB_URL" -q -c "INSERT INTO board_configs (board_id) SELECT id FROM boards WHERE id NOT IN (SELECT board_id FROM board_configs) ON CONFLICT DO NOTHING;" 2>/dev/null || true

info "Creating boards ..."
create_board "b"    "/b/ — Random"        "Anything goes. Keep it legal."          >/dev/null
create_board "tech" "/tech/ — Technology" "No consumer advice. Source your claims." >/dev/null
create_board "pol"  "/pol/ — Politics"    "Political discussion. No spam."          >/dev/null
create_board "art"  "/art/ — Artwork"     "Post your art. Critique welcome."        >/dev/null
create_board "mu"   "/mu/ — Music"        "Music discussion and production."        >/dev/null
info "Boards: /b/ /tech/ /pol/ /art/ /mu/"

# Note: Janitors moderate all boards by role — no board_owners row required.
# Only board_owner accounts need entries in board_owners.

# ── Assign board_owner to /b/ ──────────────────────────────────────────────────
info "Assigning board_owner to /b/ ..."
B_ID=$(psql "$DB_URL" -tAc "SELECT id FROM boards WHERE slug='b' LIMIT 1" 2>/dev/null || true)
BO_ID=$(psql "$DB_URL" -tAc "SELECT id FROM users WHERE username='board_owner' LIMIT 1" 2>/dev/null || true)
if [[ -n "$B_ID" && -n "$BO_ID" ]]; then
    psql "$DB_URL" -q -c "INSERT INTO board_owners (board_id, user_id) VALUES ('$B_ID','$BO_ID') ON CONFLICT DO NOTHING;" 2>/dev/null || true
    info "board_owner assigned to /b/"
fi

# ── 4. Threads + posts ────────────────────────────────────────────────────────
info "Seeding /b/ ..."
R=$(post_to_board "b" "What's everyone having for dinner? I made ramen from scratch."); T=$(echo "$R"|tid)
post_to_board "b" "Leftover pizza. No shame." "$T" >/dev/null
post_to_board "b" "Homemade ramen — post the recipe." "$T" >/dev/null
post_to_board "b" "Cereal at 9pm. Don't judge me." "$T" >/dev/null

R=$(post_to_board "b" "Post your desktop. Judging will occur."); T=$(echo "$R"|tid)
post_to_board "b" "Tiling WM, no taskbar. Minimalist life." "$T" >/dev/null
post_to_board "b" "Windows 11 + Rainmeter. Fight me." "$T" >/dev/null

R=$(post_to_board "b" "What hobby stuck this year?"); T=$(echo "$R"|tid)
post_to_board "b" "Bread baking. Relaxing once you stop caring about perfect loaves." "$T" >/dev/null
post_to_board "b" "Analog photography. Expensive but worth it." "$T" >/dev/null

info "Seeding /tech/ ..."
R=$(post_to_board "tech" "Rust 2025 edition — async traits stabilisation was long overdue. What are you building?"); T=$(echo "$R"|tid)
post_to_board "tech" "Shipping without an async runtime debate every PR. axum + tokio is solid." "$T" >/dev/null
post_to_board "tech" "Borrow checker still catches me on self-referential structs. Worth it overall." "$T" >/dev/null

R=$(post_to_board "tech" "Best self-hosted git in 2025? Forgejo vs Gitea vs Gitlab?"); T=$(echo "$R"|tid)
post_to_board "tech" "Forgejo has been more active since the drama. Switched 6 months ago, no regrets." "$T" >/dev/null
post_to_board "tech" "Gitlab RAM usage is embarrassing unless you need deep CI." "$T" >/dev/null

R=$(post_to_board "tech" "Show me your homelab. What are you running?"); T=$(echo "$R"|tid)
post_to_board "tech" "Thinkpad + Proxmox + Tailscale. Low power, gets the job done." "$T" >/dev/null
post_to_board "tech" "3x N100 mini PCs, k3s on top. Overkill? Yes. Fun? Also yes." "$T" >/dev/null

info "Seeding /pol/ ..."
R=$(post_to_board "pol" "Privacy vs convenience in consumer tech — harder to avoid surveillance capitalism."); T=$(echo "$R"|tid)
post_to_board "pol" "The friction is the point. They make privacy hard so most people don't bother." "$T" >/dev/null
post_to_board "pol" "Degoogled Android + self-hosted is feasible for non-technical users willing to spend an afternoon." "$T" >/dev/null

R=$(post_to_board "pol" "Local government is more impactful than national politics but gets 1/100th the attention."); T=$(echo "$R"|tid)
post_to_board "pol" "National politics is better entertainment. Local is zoning boards and school budgets." "$T" >/dev/null
post_to_board "pol" "Local has more direct power over daily life — taxes, building codes, policing." "$T" >/dev/null

info "Seeding /art/ ..."
R=$(post_to_board "art" "Post your current WIP. Digital, traditional, sculpture — anything."); T=$(echo "$R"|tid)
post_to_board "art" "Charcoal portrait series. Capturing expression without overworking the surface." "$T" >/dev/null
post_to_board "art" "Pixel art tileset for a game jam. 16x16 constraint is oddly liberating." "$T" >/dev/null

R=$(post_to_board "art" "Tips for getting out of art block? Three weeks, blank canvas."); T=$(echo "$R"|tid)
post_to_board "art" "Draw something bad on purpose. Breaks the paralysis." "$T" >/dev/null
post_to_board "art" "Switch medium. Digital → paper. Unfamiliarity removes expectations." "$T" >/dev/null

info "Seeding /mu/ ..."
R=$(post_to_board "mu" "What have you been listening to this week? Recommend something obscure."); T=$(echo "$R"|tid)
post_to_board "mu" "Hailu Mergia — Wede Harer Guzo. Ethiopian jazz from the 70s. Stunning." "$T" >/dev/null
post_to_board "mu" "Grouper's Dragging a Dead Deer Up a Hill again. Never gets old." "$T" >/dev/null

R=$(post_to_board "mu" "Home studio thread. DAW, interface, monitors?"); T=$(echo "$R"|tid)
post_to_board "mu" "Reaper + Focusrite 2i2 + Yamaha HS5. Great value." "$T" >/dev/null
post_to_board "mu" "Switched Ableton → Bitwig. Modular routing is worth the curve." "$T" >/dev/null

# ── 5. Sample ban ─────────────────────────────────────────────────────────────
info "Adding sample ban for dashboard preview ..."
JANITOR_TOKEN=$(curl -sf -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"janitor\",\"password\":\"$JANITOR_PASS\"}" \
    | python3 -c "import sys,json; print(json.load(sys.stdin)['token'])")
curl -sf -X POST "$BASE_URL/mod/bans" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $JANITOR_TOKEN" \
    -d '{"ip_hash":"deadbeef1234567890abcdef","reason":"Test ban — seed data","expires_at":null}' >/dev/null

# ── 6. Summary ────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  Seed complete!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════${NC}"
echo ""
echo "  Users (role → dashboard):"
echo "    admin       / admin123     → /admin/dashboard"
echo "    janitor     / janitor123   → /janitor/dashboard"
echo "    board_owner / owner123     → /board-owner/dashboard"
echo "    volunteer   / vol123       → /volunteer/dashboard"
echo "    testuser    / user123      → /auth/login (user role — no dashboard)"
echo ""
echo "  Boards:  /b/  /tech/  /pol/  /art/  /mu/"
echo ""
echo "  Try:"
echo "    $BASE_URL/overboard"
echo "    $BASE_URL/board/tech"
echo "    $BASE_URL/auth/login"
echo "    $BASE_URL/admin/dashboard        (login: admin / admin123)"
echo "    $BASE_URL/janitor/dashboard      (login: janitor / janitor123)"
echo "    $BASE_URL/board-owner/dashboard  (login: board_owner / owner123)"
echo "    $BASE_URL/volunteer/dashboard    (login: volunteer / vol123)"
echo ""
