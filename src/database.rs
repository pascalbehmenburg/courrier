use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use chrono::{DateTime, Utc};

pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct EmailStats {
    pub account_email: String,
    pub mailbox: String,
    pub count: i64,
    pub total_size_bytes: i64,
    pub last_fetch: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct FetchStatus {
    pub is_running: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub messages_fetched: i64,
    pub messages_total: Option<i64>,
}

impl Database {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Database {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "CREATE TABLE IF NOT EXISTS fetched_emails (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_email TEXT NOT NULL,
                mailbox TEXT NOT NULL,
                uid INTEGER NOT NULL,
                file_path TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                fetched_at TEXT NOT NULL,
                UNIQUE(account_email, mailbox, uid)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS fetch_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_email TEXT NOT NULL,
                mailbox TEXT NOT NULL,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                messages_fetched INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'running'
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_fetched_emails_lookup 
             ON fetched_emails(account_email, mailbox, uid)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_fetched_emails_stats 
             ON fetched_emails(account_email, mailbox)",
            [],
        )?;

        Ok(())
    }

    pub fn is_email_fetched(&self, account_email: &str, mailbox: &str, uid: u32) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT 1 FROM fetched_emails 
             WHERE account_email = ?1 AND mailbox = ?2 AND uid = ?3 
             LIMIT 1"
        )?;
        let exists = stmt.exists(params![account_email, mailbox, uid])?;
        Ok(exists)
    }

    pub fn mark_email_fetched(
        &self,
        account_email: &str,
        mailbox: &str,
        uid: u32,
        file_path: &PathBuf,
        size_bytes: usize,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO fetched_emails 
             (account_email, mailbox, uid, file_path, size_bytes, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                account_email,
                mailbox,
                uid,
                file_path.to_string_lossy(),
                size_bytes as i64,
                now
            ],
        )?;
        Ok(())
    }

    pub fn get_fetched_uids(&self, account_email: &str, mailbox: &str) -> Result<Vec<u32>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT uid FROM fetched_emails 
             WHERE account_email = ?1 AND mailbox = ?2"
        )?;
        let uids: Result<Vec<u32>, _> = stmt
            .query_map(params![account_email, mailbox], |row| {
                Ok(row.get::<_, i64>(0)? as u32)
            })?
            .collect();
        Ok(uids?)
    }

    pub fn get_stats(&self) -> Result<Vec<EmailStats>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT 
                account_email,
                mailbox,
                COUNT(*) as count,
                SUM(size_bytes) as total_size_bytes,
                MAX(fetched_at) as last_fetch
             FROM fetched_emails
             GROUP BY account_email, mailbox
             ORDER BY account_email, mailbox"
        )?;

        let stats: Result<Vec<EmailStats>, _> = stmt
            .query_map([], |row| {
                let account_email: String = row.get(0)?;
                let mailbox: String = row.get(1)?;
                let count: i64 = row.get(2)?;
                let total_size_bytes: Option<i64> = row.get(3)?;
                let last_fetch_str: Option<String> = row.get(4)?;

                let last_fetch = last_fetch_str
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                Ok(EmailStats {
                    account_email,
                    mailbox,
                    count,
                    total_size_bytes: total_size_bytes.unwrap_or(0),
                    last_fetch,
                })
            })?
            .collect();

        Ok(stats?)
    }

    pub fn get_total_stats(&self) -> Result<(i64, i64)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT 
                COUNT(*) as total_count,
                SUM(size_bytes) as total_size_bytes
             FROM fetched_emails"
        )?;

        let row = stmt.query_row([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            ))
        })?;

        Ok(row)
    }

    pub fn start_fetch_history(
        &self,
        account_email: &str,
        mailbox: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO fetch_history 
             (account_email, mailbox, started_at, status)
             VALUES (?1, ?2, ?3, 'running')",
            params![account_email, mailbox, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn complete_fetch_history(
        &self,
        id: i64,
        messages_fetched: i64,
        status: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE fetch_history 
             SET completed_at = ?1, messages_fetched = ?2, status = ?3
             WHERE id = ?4",
            params![now, messages_fetched, status, id],
        )?;
        Ok(())
    }

    pub fn get_latest_fetch_status(&self) -> Result<Option<FetchStatus>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT started_at, completed_at, messages_fetched, status
             FROM fetch_history
             ORDER BY started_at DESC
             LIMIT 1"
        )?;

        let mut rows = stmt.query_map([], |row| {
            let started_at_str: String = row.get(0)?;
            let completed_at_str: Option<String> = row.get(1)?;
            let messages_fetched: i64 = row.get(2)?;
            let status: String = row.get(3)?;

            let started_at = DateTime::parse_from_rfc3339(&started_at_str)
                .ok()
                .map(|dt| dt.with_timezone(&Utc));

            let is_running = completed_at_str.is_none() && status == "running";

            Ok(FetchStatus {
                is_running,
                started_at,
                messages_fetched,
                messages_total: None,
            })
        })?;

        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }
}

