use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct Database {
    pub conn: Arc<Mutex<Connection>>,
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
    pub started_at: Option<DateTime<Utc>>,
    pub messages_fetched: i64,
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

    pub fn mark_email_fetched(
        &self,
        account_email: &str,
        mailbox: &str,
        uid: u32,
        file_path: &Path,
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
             WHERE account_email = ?1 AND mailbox = ?2",
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
             ORDER BY account_email, mailbox",
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
             FROM fetched_emails",
        )?;

        let row = stmt.query_row([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            ))
        })?;

        Ok(row)
    }

    pub fn get_latest_fetch_status(&self) -> Result<Option<FetchStatus>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT started_at, completed_at, messages_fetched, status
             FROM fetch_history
             ORDER BY started_at DESC
             LIMIT 1",
        )?;

        let mut rows = stmt.query_map([], |row| {
            let started_at_str: String = row.get(0)?;
            let messages_fetched: i64 = row.get(2)?;

            let started_at = DateTime::parse_from_rfc3339(&started_at_str)
                .ok()
                .map(|dt| dt.with_timezone(&Utc));

            Ok(FetchStatus {
                started_at,
                messages_fetched,
            })
        })?;

        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    pub fn get_latest_completed_at(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let completed_at = conn
            .query_row(
                "SELECT completed_at FROM fetch_history ORDER BY started_at DESC LIMIT 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
        Ok(completed_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_db() -> Database {
        Database::new(":memory:").unwrap()
    }

    #[test]
    fn test_database_creation() {
        let db = create_test_db();
        let conn = db.conn.lock().unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"fetched_emails".to_string()));
        assert!(tables.contains(&"fetch_history".to_string()));
    }

    #[test]
    fn test_mark_email_fetched() {
        let db = create_test_db();
        let file_path = PathBuf::from("/emails/test/1.eml");

        db.mark_email_fetched("user@example.com", "INBOX", 1, &file_path, 1024)
            .unwrap();

        let uids = db.get_fetched_uids("user@example.com", "INBOX").unwrap();
        assert_eq!(uids, vec![1]);
    }

    #[test]
    fn test_mark_email_fetched_multiple() {
        let db = create_test_db();

        db.mark_email_fetched(
            "user@example.com",
            "INBOX",
            1,
            &PathBuf::from("/emails/1.eml"),
            1024,
        )
        .unwrap();
        db.mark_email_fetched(
            "user@example.com",
            "INBOX",
            2,
            &PathBuf::from("/emails/2.eml"),
            2048,
        )
        .unwrap();
        db.mark_email_fetched(
            "user@example.com",
            "INBOX",
            3,
            &PathBuf::from("/emails/3.eml"),
            512,
        )
        .unwrap();

        let mut uids = db.get_fetched_uids("user@example.com", "INBOX").unwrap();
        uids.sort();
        assert_eq!(uids, vec![1, 2, 3]);
    }

    #[test]
    fn test_mark_email_fetched_replace() {
        let db = create_test_db();

        db.mark_email_fetched(
            "user@example.com",
            "INBOX",
            1,
            &PathBuf::from("/emails/1.eml"),
            1024,
        )
        .unwrap();
        db.mark_email_fetched(
            "user@example.com",
            "INBOX",
            1,
            &PathBuf::from("/emails/1_new.eml"),
            2048,
        )
        .unwrap();

        let uids = db.get_fetched_uids("user@example.com", "INBOX").unwrap();
        assert_eq!(uids.len(), 1);
    }

    #[test]
    fn test_get_fetched_uids_different_mailboxes() {
        let db = create_test_db();

        db.mark_email_fetched(
            "user@example.com",
            "INBOX",
            1,
            &PathBuf::from("/emails/inbox/1.eml"),
            1024,
        )
        .unwrap();
        db.mark_email_fetched(
            "user@example.com",
            "Sent",
            1,
            &PathBuf::from("/emails/sent/1.eml"),
            2048,
        )
        .unwrap();

        let inbox_uids = db.get_fetched_uids("user@example.com", "INBOX").unwrap();
        let sent_uids = db.get_fetched_uids("user@example.com", "Sent").unwrap();

        assert_eq!(inbox_uids, vec![1]);
        assert_eq!(sent_uids, vec![1]);
    }

    #[test]
    fn test_get_fetched_uids_different_accounts() {
        let db = create_test_db();

        db.mark_email_fetched(
            "user1@example.com",
            "INBOX",
            1,
            &PathBuf::from("/emails/user1/1.eml"),
            1024,
        )
        .unwrap();
        db.mark_email_fetched(
            "user2@example.com",
            "INBOX",
            2,
            &PathBuf::from("/emails/user2/2.eml"),
            2048,
        )
        .unwrap();

        let user1_uids = db.get_fetched_uids("user1@example.com", "INBOX").unwrap();
        let user2_uids = db.get_fetched_uids("user2@example.com", "INBOX").unwrap();

        assert_eq!(user1_uids, vec![1]);
        assert_eq!(user2_uids, vec![2]);
    }

    #[test]
    fn test_get_fetched_uids_empty() {
        let db = create_test_db();
        let uids = db
            .get_fetched_uids("nonexistent@example.com", "INBOX")
            .unwrap();
        assert!(uids.is_empty());
    }

    #[test]
    fn test_get_stats() {
        let db = create_test_db();

        db.mark_email_fetched(
            "user@example.com",
            "INBOX",
            1,
            &PathBuf::from("/emails/1.eml"),
            1000,
        )
        .unwrap();
        db.mark_email_fetched(
            "user@example.com",
            "INBOX",
            2,
            &PathBuf::from("/emails/2.eml"),
            2000,
        )
        .unwrap();

        let stats = db.get_stats().unwrap();

        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].account_email, "user@example.com");
        assert_eq!(stats[0].mailbox, "INBOX");
        assert_eq!(stats[0].count, 2);
        assert_eq!(stats[0].total_size_bytes, 3000);
        assert!(stats[0].last_fetch.is_some());
    }

    #[test]
    fn test_get_stats_multiple_mailboxes() {
        let db = create_test_db();

        db.mark_email_fetched(
            "user@example.com",
            "INBOX",
            1,
            &PathBuf::from("/emails/inbox/1.eml"),
            1000,
        )
        .unwrap();
        db.mark_email_fetched(
            "user@example.com",
            "Sent",
            1,
            &PathBuf::from("/emails/sent/1.eml"),
            500,
        )
        .unwrap();

        let stats = db.get_stats().unwrap();

        assert_eq!(stats.len(), 2);
    }

    #[test]
    fn test_get_stats_empty() {
        let db = create_test_db();
        let stats = db.get_stats().unwrap();
        assert!(stats.is_empty());
    }

    #[test]
    fn test_get_total_stats() {
        let db = create_test_db();

        db.mark_email_fetched(
            "user1@example.com",
            "INBOX",
            1,
            &PathBuf::from("/emails/1.eml"),
            1000,
        )
        .unwrap();
        db.mark_email_fetched(
            "user2@example.com",
            "INBOX",
            1,
            &PathBuf::from("/emails/2.eml"),
            2000,
        )
        .unwrap();

        let (total_count, total_size) = db.get_total_stats().unwrap();

        assert_eq!(total_count, 2);
        assert_eq!(total_size, 3000);
    }

    #[test]
    fn test_get_total_stats_empty() {
        let db = create_test_db();
        let (total_count, total_size) = db.get_total_stats().unwrap();

        assert_eq!(total_count, 0);
        assert_eq!(total_size, 0);
    }

    #[test]
    fn test_get_latest_fetch_status_empty() {
        let db = create_test_db();
        let status = db.get_latest_fetch_status().unwrap();
        assert!(status.is_none());
    }

    #[test]
    fn test_get_latest_fetch_status() {
        let db = create_test_db();
        let now = Utc::now().to_rfc3339();

        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO fetch_history (account_email, mailbox, started_at, messages_fetched, status) VALUES (?1, ?2, ?3, ?4, ?5)",
                params!["user@example.com", "INBOX", now, 10, "completed"],
            ).unwrap();
        }

        let status = db.get_latest_fetch_status().unwrap();
        assert!(status.is_some());
        let status = status.unwrap();
        assert_eq!(status.messages_fetched, 10);
        assert!(status.started_at.is_some());
    }

    #[test]
    fn test_get_latest_completed_at_empty() {
        let db = create_test_db();
        let completed_at = db.get_latest_completed_at().unwrap();
        assert!(completed_at.is_none());
    }

    #[test]
    fn test_get_latest_completed_at() {
        let db = create_test_db();
        let now = Utc::now().to_rfc3339();
        let completed = Utc::now().to_rfc3339();

        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO fetch_history (account_email, mailbox, started_at, completed_at, messages_fetched, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params!["user@example.com", "INBOX", now, completed.clone(), 5, "completed"],
            ).unwrap();
        }

        let result = db.get_latest_completed_at().unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), completed);
    }
}
