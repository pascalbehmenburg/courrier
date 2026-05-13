//! Database schema. Statements are idempotent (CREATE IF NOT EXISTS) so
//! `init_schema` can run on every startup.

pub const SCHEMA: &[&str] = &[
    // Accounts: user-managed IMAP connections. password_ciphertext is
    // base64(nonce || ciphertext || tag) under the AES-GCM key from env.
    "CREATE TABLE IF NOT EXISTS accounts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        label TEXT NOT NULL,
        email TEXT NOT NULL UNIQUE,
        username TEXT NOT NULL,
        password_ciphertext TEXT NOT NULL,
        host TEXT NOT NULL,
        port INTEGER NOT NULL DEFAULT 993,
        provider_id TEXT NOT NULL DEFAULT 'custom',
        sync_interval_seconds INTEGER,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    // Per-account, per-mailbox UID tracking + on-disk path of the .eml.
    "CREATE TABLE IF NOT EXISTS fetched_emails (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        mailbox TEXT NOT NULL,
        uid INTEGER NOT NULL,
        file_path TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        fetched_at TEXT NOT NULL,
        UNIQUE(account_id, mailbox, uid)
    )",
    // Per-account fetch runs. account_id NULL means "all accounts" (manual
    // sync-all triggered from the UI).
    "CREATE TABLE IF NOT EXISTS fetch_runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        account_id INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
        started_at TEXT NOT NULL,
        completed_at TEXT,
        messages_fetched INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'running',
        error TEXT
    )",
    // Parsed mail metadata. Body lives in body_text (utf-8); raw bytes
    // remain on disk in file_path. fetched_email_id is the link.
    "CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        fetched_email_id INTEGER NOT NULL UNIQUE REFERENCES fetched_emails(id) ON DELETE CASCADE,
        account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        mailbox TEXT NOT NULL,
        message_id TEXT,
        subject TEXT,
        from_addr TEXT,
        from_name TEXT,
        to_addrs TEXT,
        cc_addrs TEXT,
        date_utc TEXT,
        body_text TEXT,
        is_forwarded INTEGER NOT NULL DEFAULT 0,
        forwarded_from TEXT,
        forwarded_from_domain TEXT,
        original_sender_domain TEXT,
        original_sender_addr TEXT,
        size_bytes INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE INDEX IF NOT EXISTS idx_fetched_emails_account
        ON fetched_emails(account_id, mailbox)",
    "CREATE INDEX IF NOT EXISTS idx_messages_account ON messages(account_id)",
    "CREATE INDEX IF NOT EXISTS idx_messages_from ON messages(from_addr)",
    "CREATE INDEX IF NOT EXISTS idx_messages_date ON messages(date_utc)",
    "CREATE INDEX IF NOT EXISTS idx_messages_forwarded
        ON messages(account_id, is_forwarded, forwarded_from)",
    "CREATE INDEX IF NOT EXISTS idx_fetch_runs_account
        ON fetch_runs(account_id, started_at DESC)",
    // FTS5 virtual table mirroring messages — populated via triggers below.
    "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
        subject, from_addr, from_name, to_addrs, body_text,
        content='messages', content_rowid='id', tokenize='porter unicode61'
    )",
    "CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
        INSERT INTO messages_fts(rowid, subject, from_addr, from_name, to_addrs, body_text)
        VALUES (new.id, new.subject, new.from_addr, new.from_name, new.to_addrs, new.body_text);
    END",
    "CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
        INSERT INTO messages_fts(messages_fts, rowid, subject, from_addr, from_name, to_addrs, body_text)
        VALUES ('delete', old.id, old.subject, old.from_addr, old.from_name, old.to_addrs, old.body_text);
    END",
    "CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
        INSERT INTO messages_fts(messages_fts, rowid, subject, from_addr, from_name, to_addrs, body_text)
        VALUES ('delete', old.id, old.subject, old.from_addr, old.from_name, old.to_addrs, old.body_text);
        INSERT INTO messages_fts(rowid, subject, from_addr, from_name, to_addrs, body_text)
        VALUES (new.id, new.subject, new.from_addr, new.from_name, new.to_addrs, new.body_text);
    END",
];
