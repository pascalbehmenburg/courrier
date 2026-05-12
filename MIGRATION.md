# Migrating to Courrier 0.2 (DB-managed accounts)

Pre-0.2 used `Config.toml` + an `emails/` directory and stored fetch state in
a SQLite DB keyed by `account_email`. 0.2 stores accounts in the DB (with
AES-GCM-encrypted passwords) and keys fetch state by `account_id`. The
on-disk `.eml` layout — `<storage>/<sanitize(email)>/<sanitize(mailbox)>/<uid>.eml` —
is unchanged, so existing files can be carried over verbatim.

The schema is **not** in-place upgradable: `CREATE TABLE IF NOT EXISTS` is
a no-op when columns differ, so reusing the old `courrier.db` will produce
silent failures. Start with a fresh DB and run the migrator.

## Inputs

- Old `Config.toml` (the pre-0.2 server config with `[[servers]]` blocks).
- Old `emails/` directory (the .eml tree the legacy fetcher wrote).
- A new `COURRIER_ENCRYPTION_KEY` — base64 of 32 random bytes. **Generate
  it once and keep it safe.** Losing it bricks every stored IMAP password.

```sh
head -c 32 /dev/urandom | base64
```

## One-shot migrator

A `courrier-migrate` binary in `crates/migrate/` performs the import:

1. Inserts an `accounts` row per (server, account) in `Config.toml`,
   encrypting each password with `COURRIER_ENCRYPTION_KEY`.
2. Walks the `emails/` tree and inserts a `fetched_emails` row per `.eml`
   file (account_id + mailbox + uid + path + size). After this step the new
   server treats those UIDs as already-fetched and skips them on the next
   sync.
3. Host → provider_id mapping (`imap.gmail.com` → `gmail`, etc.) is
   applied so the UI shows the right provider icon; unrecognised hosts
   become `custom`.

The server's existing `backfill_parser` does the rest at next boot: it
reads each .eml, parses it, and inserts a `messages` row + FTS entries.

## Step-by-step (for another agent)

Assumptions: legacy data lives at `~/courrier-old/` on the homeserver,
containing `Config.toml` and `emails/`. The new deployment runs in Docker
at `~/courrier/` with bind-mount `./data → /data`. Adapt the paths.

### 0. Pre-flight on the homeserver

```sh
# Snapshot the legacy data — migration writes a new DB but leaves the .eml
# tree intact; copy first to be safe.
cp -a ~/courrier-old ~/courrier-old.bak
```

### 1. Prepare the new deploy directory

```sh
mkdir -p ~/courrier && cd ~/courrier
# docker-compose.yml lives here. Use docker-compose.example.yml from the
# repo as the starting point.
mkdir -p data
sudo chown -R 10001:10001 data            # matches the in-container user
mkdir -p data/emails
cp -a ~/courrier-old/emails/. data/emails/
sudo chown -R 10001:10001 data/emails
```

### 2. Generate the encryption key

```sh
KEY=$(head -c 32 /dev/urandom | base64)
echo "$KEY" > ~/.courrier.key       # back it up off-host too
chmod 600 ~/.courrier.key
```

Place `$KEY` into `docker-compose.yml` as the value of
`COURRIER_ENCRYPTION_KEY`.

### 3. Run the migrator

The migrator needs `cargo` on the host (one-time tool). Build it from a
checkout of the repo:

```sh
git clone https://github.com/pascalbehmenburg/courrier.git /tmp/courrier-src
cd /tmp/courrier-src
cargo build --release -p courrier-migrate

# Run against the new data dir. The file paths recorded in fetched_emails
# are stored verbatim, so use the path the *server* will see — which means
# the host path that maps into /data/emails inside the container. Then
# write the DB to the spot the container will read.
COURRIER_ENCRYPTION_KEY="$KEY" \
  ./target/release/courrier-migrate \
    --config ~/courrier-old/Config.toml \
    --emails ~/courrier/data/emails \
    --db    ~/courrier/data/courrier.db

# Fix ownership: the binary ran as the host user, the container runs as
# UID 10001.
sudo chown 10001:10001 ~/courrier/data/courrier.db
```

The migrator prints one line per account inserted, then one line per
mailbox imported with its file count, then a summary.

**Important:** the paths in `fetched_emails.file_path` are stored as
written. If you give `--emails ~/courrier/data/emails` then the rows hold
`/home/you/courrier/data/emails/foo/INBOX/1.eml` — fine, because that
host path is bind-mounted to the same string at `/data/emails/...` only
when the host paths and container paths happen to differ. **Safer**: run
the migrator inside the same path layout the container sees, e.g. with
`--emails /data/emails --db /data/courrier.db` from a directory that
already mirrors `/data/`, or run the migrator inside the container (see
"Alternative: run inside a container" below).

### 4. Boot the new server

```sh
cd ~/courrier
docker compose pull            # ghcr.io/pascalbehmenburg/courrier:latest
docker compose up -d
docker compose logs -f courrier
```

On first boot you should see lines like:

```
Storage: /data/emails
Database: /data/courrier.db
Backfilled NNN parsed message(s) at startup
Sync scheduler started (1-minute granularity)
Initial sync started for N account(s)
```

The `Backfilled NNN` line is the migration paying off — the parser is
populating `messages` + FTS from disk without IMAP. The initial sync that
follows will hit IMAP only for **new** UIDs (none, if the migration was
recent and complete).

### 5. Verify

- `curl http://localhost:3000/api/health` → 200.
- Open `http://homeserver:3000/`, confirm accounts appear in the sidebar
  with the right host/provider.
- Click "Sync now" on one account; it should report 0 new messages on a
  freshly migrated mailbox.
- Search for a known subject in the UI — FTS should return it.

### Rollback

If anything looks wrong:

```sh
docker compose down
rm ~/courrier/data/courrier.db        # drop only the DB, keep emails/
# Re-run from step 3 with --config + --emails. The .eml files are never
# mutated by the migrator.
```

To go back to pre-0.2 entirely: `rm -rf ~/courrier && mv ~/courrier-old.bak ~/courrier-old` and run the old binary as before.

## Alternative: run the migrator inside a container

If you don't want cargo on the host, build a thin throw-away image:

```sh
docker run --rm -it \
  -v ~/courrier-old/Config.toml:/in/Config.toml:ro \
  -v ~/courrier/data:/data \
  -e COURRIER_ENCRYPTION_KEY="$KEY" \
  -w /src \
  -v /tmp/courrier-src:/src \
  rust:1.84-bookworm \
  bash -c "cargo run --release -p courrier-migrate -- \
    --config /in/Config.toml \
    --emails /data/emails \
    --db    /data/courrier.db"

sudo chown -R 10001:10001 ~/courrier/data
```

This guarantees the `fetched_emails.file_path` values match exactly what
the server sees at runtime (`/data/emails/...`).

## What does not migrate

- The legacy `fetch_history` table. New runs are tracked in `fetch_runs`;
  there is no historical view of pre-migration syncs in the UI.
- `Config.toml`'s `fetch_interval_seconds` is global and per-account in
  the new schema. The migrator leaves `sync_interval_seconds` NULL on
  every imported account; set it from the UI if you want auto-sync.
- Sanitization caveat: mailbox names containing `/` were rewritten to `_`
  on disk by the legacy fetcher. The migrator reads the directory name
  literally, so `Notes/Personal` is stored as `Notes_Personal` in
  `fetched_emails.mailbox`. The next IMAP sync (which lists the original
  `Notes/Personal`) will see this as a new mailbox and re-fetch its
  contents into `Notes_Personal/` (idempotent on disk, but it does cost
  one round of bandwidth). If you have no nested mailbox names this
  does not apply.
