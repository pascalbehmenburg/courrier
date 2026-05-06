# Courrier

A self-hosted email backup, search, and analysis service. Connects to any number
of IMAP accounts, downloads each message as a `.eml` file, parses the headers
and body into a SQLite database, and exposes everything through a modern web
dashboard. Ships as a single Docker image for the server, plus an optional
Tauri desktop app that talks to the same backend.

## Features

- **Dashboard-managed accounts** — add/edit/remove IMAP connections from the UI;
  no config files, no redeploys. Provider presets for iCloud, Gmail / Workspace,
  Outlook / Microsoft 365, Yahoo, Fastmail, web.de, GMX, and a custom option.
- **Encrypted credentials at rest** — passwords are AES-GCM-256 encrypted
  before they touch the database. The key lives in an env var.
- **Per-account sync** — sync now / sync all buttons, configurable auto-sync
  interval per account, in-flight progress + last-error visible per account.
- **Full-text search** — SQLite FTS5 index over subject, from, to, body. The
  search box accepts FTS5 syntax (`"phrase"`, `term1 AND term2`,
  `column:value`).
- **Forwarding analysis** — detects forwarded mail (Resent-* / X-Forwarded-*
  headers, body sentinels for Gmail / Apple / Outlook / German Outlook) and
  surfaces the original sender's domain. Answers questions like *"how many
  mails were forwarded from `ambien@web.de`, and which domains did they
  originate from?"*
- **Analytics** — top senders, sender-domain breakdown, mailbox distribution,
  daily traffic timeline, all scopable to one account or to everything.
- **Two clients, one backend** — the same React + shadcn UI is served as a
  web app by the axum binary *and* packaged as a native desktop app by Tauri 2
  (the desktop app can point at a remote Docker deployment).

## Architecture

```
crates/
  core/     # all durable logic: encryption, DB, IMAP fetcher,
            # mail parser, FTS5 search, analytics, sync coordinator
  server/   # axum HTTP API; embeds the React SPA via rust-embed
desktop/    # Vite + React + Tailwind + shadcn-style UI
  src-tauri/  # Tauri 2 native shell loading the same SPA
```

The server is the single source of truth — both the embedded web UI and the
Tauri desktop app are thin REST clients of the same `/api/*` endpoints.

## Quick start (Docker)

```bash
# Generate the encryption key once and stash it somewhere safe.
KEY=$(head -c 32 /dev/urandom | base64)

mkdir -p data && sudo chown -R 10001:10001 data

docker run -d --name courrier \
  -p 3000:3000 \
  -v "$(pwd)/data:/data" \
  -e COURRIER_ENCRYPTION_KEY="$KEY" \
  ghcr.io/pascalbehmenburg/courrier:latest
```

Then open http://localhost:3000 and add an account from the dashboard.

The image bundles both the API and the web UI. To run the same setup with
docker compose, copy `docker-compose.example.yml` to `docker-compose.yml`
and replace the `COURRIER_ENCRYPTION_KEY` placeholder.

## Configuration

All process-wide settings come from environment variables. Per-account
settings (host, port, sync interval, …) live in the database and are managed
from the UI.

| Variable                    | Default              | Notes                                 |
| --------------------------- | -------------------- | ------------------------------------- |
| `COURRIER_ENCRYPTION_KEY`   | _(required)_         | Base64 of 32 random bytes (AES-GCM)   |
| `COURRIER_DB_PATH`          | `courrier.db`        | SQLite file                           |
| `COURRIER_STORAGE_PATH`     | `emails`             | Directory for `.eml` files            |
| `COURRIER_BIND_ADDR`        | `127.0.0.1:3000`     | Server bind (Docker overrides to all) |
| `COURRIER_FETCH_ON_STARTUP` | `true`               | Trigger sync-all on boot              |
| `RUST_LOG`                  | `info`               | Standard `tracing` env-filter syntax  |

### Generating the encryption key

```bash
head -c 32 /dev/urandom | base64
```

**Losing this key means losing access to every stored IMAP password.** Back it
up next to the database, or hand it to your secret manager.

## Local development

You'll need Rust, Node 22, and pnpm. The desktop app expects Tauri 2 system
deps if you want to run it natively (see <https://v2.tauri.app/start/prerequisites/>).

```bash
# Backend (terminal 1)
export COURRIER_ENCRYPTION_KEY=$(head -c 32 /dev/urandom | base64)
cargo run -p courrier-server

# Frontend with HMR (terminal 2)
cd desktop
pnpm install
pnpm dev
# → http://localhost:5173, /api proxied to 127.0.0.1:3000
```

For the Tauri desktop app:

```bash
cd desktop
pnpm tauri dev      # native window, hot-reloaded
pnpm tauri build    # native installers in src-tauri/target/release/bundle
```

When the desktop app talks to a *remote* backend, point it at the URL on the
Settings page; that value persists per device.

## API surface

Sketch — see `crates/server/src/routes/` for the full set.

```
GET    /api/health
GET    /api/providers

GET    /api/accounts
POST   /api/accounts
GET    /api/accounts/:id
PUT    /api/accounts/:id
DELETE /api/accounts/:id
POST   /api/accounts/:id/test          # smoke-test the IMAP login

POST   /api/sync                       # all enabled accounts
POST   /api/sync/:account_id           # one account
GET    /api/sync/status                # per-account in-flight + latest run

GET    /api/messages?account_id=&mailbox=&limit=&offset=
GET    /api/messages/:id
GET    /api/messages/:id/raw           # raw .eml bytes

GET    /api/search?q=&account_id=&limit=

GET    /api/analytics/overview?account_id=
GET    /api/analytics/top-senders
GET    /api/analytics/top-sender-domains
GET    /api/analytics/forwarding
GET    /api/analytics/timeline?days=
GET    /api/analytics/mailboxes
```

State-changing requests must include the `X-Requested-With` header (the SPA
sets it automatically). CORS is permissive so the Tauri app can talk to a
remote deployment.

## License

MIT — see `LICENSE`.
