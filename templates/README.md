# templates/

This directory is a **reference copy** of `crates/api-adapters/templates/`.

## Which directory is canonical?

`crates/api-adapters/templates/` — that is the directory Askama compiles.  
Any change to a template **must be made in `crates/api-adapters/templates/`**.

## What is this directory for?

This root copy exists so that contributors can browse templates from the project
root without navigating into the crate. It is kept in sync manually; if you see
a discrepancy, the crate version wins.

## Syncing

```bash
# After changing any template in crates/api-adapters/templates/:
cp crates/api-adapters/templates/*.html templates/
```

## Template hierarchy

| Template | Route | Notes |
|---|---|---|
| `base.html` | — | Extended by all other templates |
| `overboard.html` | `GET /overboard` | |
| `board.html` | `GET /board/:slug` | |
| `catalog.html` | `GET /board/:slug/catalog` | |
| `thread.html` | `GET /board/:slug/thread/:id` | |
| `login.html` | `GET /auth/login` | |
| `dashboard.html` | `GET /{role}/dashboard` | **Unified** — all roles share this template. Sections show/hide based on data presence. |
| `board_owner_dashboard.html` | `GET /board/:slug/dashboard` | Per-board config UI. Separate from the unified dashboard. |
| `mod_flags.html` | `GET /mod/flags` | |
| `mod_bans.html` | `GET /mod/bans` | |

## Unified dashboard

All roles use `dashboard.html` via a single `DashboardTemplate` struct. The handler
for each role populates only the data that role is authorised to see — empty `Vec`
and `None` fields cause sections to be hidden. The template never inspects the
role name directly.

`/board/:slug/dashboard` (`board_owner_dashboard.html`) is intentionally **separate**
— it is a deep-dive configuration surface for a single board, not a role overview.
