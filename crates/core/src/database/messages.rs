//! Stored mail messages and the per-mailbox UID index.

use super::Database;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;
use std::path::Path;

/// Newly fetched .eml on disk that hasn't been parsed yet. The parser
/// will read the file and `INSERT INTO messages`.
#[derive(Debug, Clone)]
pub struct PendingFetch {
    pub fetched_email_id: i64,
    pub account_id: i64,
    pub mailbox: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: i64,
    pub fetched_email_id: i64,
    pub account_id: i64,
    pub mailbox: String,
    pub message_id: Option<String>,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub from_name: Option<String>,
    pub to_addrs: Option<String>,
    pub cc_addrs: Option<String>,
    pub date_utc: Option<DateTime<Utc>>,
    pub body_text: Option<String>,
    pub is_forwarded: bool,
    pub forwarded_from: Option<String>,
    pub forwarded_from_domain: Option<String>,
    pub original_sender_domain: Option<String>,
    pub size_bytes: i64,
}

/// Lightweight projection used for list views (no body, no header dump).
#[derive(Debug, Clone, Serialize)]
pub struct MessageSummary {
    pub id: i64,
    pub mailbox: String,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub from_name: Option<String>,
    pub date_utc: Option<DateTime<Utc>>,
    pub is_forwarded: bool,
    pub forwarded_from: Option<String>,
    pub size_bytes: i64,
}

/// Row shape for INSERT — caller fills this from the parser.
#[derive(Debug, Clone, Default)]
pub struct MessageRow {
    pub fetched_email_id: i64,
    pub account_id: i64,
    pub mailbox: String,
    pub message_id: Option<String>,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub from_name: Option<String>,
    pub to_addrs: Option<String>,
    pub cc_addrs: Option<String>,
    pub date_utc: Option<DateTime<Utc>>,
    pub body_text: Option<String>,
    pub is_forwarded: bool,
    pub forwarded_from: Option<String>,
    pub forwarded_from_domain: Option<String>,
    pub original_sender_domain: Option<String>,
    pub size_bytes: i64,
}

fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn row_to_message(row: &Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        id: row.get("id")?,
        fetched_email_id: row.get("fetched_email_id")?,
        account_id: row.get("account_id")?,
        mailbox: row.get("mailbox")?,
        message_id: row.get("message_id")?,
        subject: row.get("subject")?,
        from_addr: row.get("from_addr")?,
        from_name: row.get("from_name")?,
        to_addrs: row.get("to_addrs")?,
        cc_addrs: row.get("cc_addrs")?,
        date_utc: parse_dt(row.get("date_utc")?),
        body_text: row.get("body_text")?,
        is_forwarded: row.get::<_, i64>("is_forwarded")? != 0,
        forwarded_from: row.get("forwarded_from")?,
        forwarded_from_domain: row.get("forwarded_from_domain")?,
        original_sender_domain: row.get("original_sender_domain")?,
        size_bytes: row.get("size_bytes")?,
    })
}

fn row_to_summary(row: &Row<'_>) -> rusqlite::Result<MessageSummary> {
    Ok(MessageSummary {
        id: row.get("id")?,
        mailbox: row.get("mailbox")?,
        subject: row.get("subject")?,
        from_addr: row.get("from_addr")?,
        from_name: row.get("from_name")?,
        date_utc: parse_dt(row.get("date_utc")?),
        is_forwarded: row.get::<_, i64>("is_forwarded")? != 0,
        forwarded_from: row.get("forwarded_from")?,
        size_bytes: row.get("size_bytes")?,
    })
}

impl Database {
    pub async fn mark_email_fetched(
        &self,
        account_id: i64,
        mailbox: &str,
        uid: u32,
        file_path: &Path,
        size_bytes: usize,
    ) -> Result<i64> {
        let mailbox = mailbox.to_string();
        let file_path = file_path.to_string_lossy().into_owned();
        let size_bytes = size_bytes as i64;
        self.run(move |conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT OR REPLACE INTO fetched_emails
                    (account_id, mailbox, uid, file_path, size_bytes, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![account_id, mailbox, uid, file_path, size_bytes, now],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
    }

    pub async fn fetched_uids(&self, account_id: i64, mailbox: &str) -> Result<Vec<u32>> {
        let mailbox = mailbox.to_string();
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT uid FROM fetched_emails
                 WHERE account_id = ?1 AND mailbox = ?2",
            )?;
            let uids: Result<Vec<u32>, _> = stmt
                .query_map(params![account_id, mailbox], |row| {
                    Ok(row.get::<_, i64>(0)? as u32)
                })?
                .collect();
            Ok(uids?)
        })
        .await
    }

    pub async fn upsert_message(&self, row: MessageRow) -> Result<i64> {
        self.run(move |conn| {
            // INSERT OR REPLACE on a UNIQUE column requires care: with the
            // FTS5 triggers we attached, OR REPLACE would emit a delete then
            // an insert, double-firing the index. Do an explicit upsert
            // via ON CONFLICT instead.
            let date_utc_str = row.date_utc.map(|d| d.to_rfc3339());
            conn.execute(
                "INSERT INTO messages (
                    fetched_email_id, account_id, mailbox, message_id,
                    subject, from_addr, from_name, to_addrs, cc_addrs,
                    date_utc, body_text,
                    is_forwarded, forwarded_from, forwarded_from_domain,
                    original_sender_domain, size_bytes
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                ON CONFLICT(fetched_email_id) DO UPDATE SET
                    subject = excluded.subject,
                    from_addr = excluded.from_addr,
                    from_name = excluded.from_name,
                    to_addrs = excluded.to_addrs,
                    cc_addrs = excluded.cc_addrs,
                    date_utc = excluded.date_utc,
                    body_text = excluded.body_text,
                    is_forwarded = excluded.is_forwarded,
                    forwarded_from = excluded.forwarded_from,
                    forwarded_from_domain = excluded.forwarded_from_domain,
                    original_sender_domain = excluded.original_sender_domain,
                    size_bytes = excluded.size_bytes",
                params![
                    row.fetched_email_id,
                    row.account_id,
                    row.mailbox,
                    row.message_id,
                    row.subject,
                    row.from_addr,
                    row.from_name,
                    row.to_addrs,
                    row.cc_addrs,
                    date_utc_str,
                    row.body_text,
                    row.is_forwarded as i64,
                    row.forwarded_from,
                    row.forwarded_from_domain,
                    row.original_sender_domain,
                    row.size_bytes,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
    }

    pub async fn list_messages(
        &self,
        account_id: Option<i64>,
        mailbox: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MessageSummary>> {
        self.run(move |conn| {
            let mut sql = String::from(
                "SELECT id, mailbox, subject, from_addr, from_name, date_utc,
                        is_forwarded, forwarded_from, size_bytes
                 FROM messages WHERE 1=1",
            );
            if account_id.is_some() {
                sql.push_str(" AND account_id = ?1");
            }
            if mailbox.is_some() {
                sql.push_str(if account_id.is_some() {
                    " AND mailbox = ?2"
                } else {
                    " AND mailbox = ?1"
                });
            }
            sql.push_str(" ORDER BY date_utc DESC NULLS LAST LIMIT ? OFFSET ?");
            let mut stmt = conn.prepare(&sql)?;

            let rows: Result<Vec<MessageSummary>, _> = match (account_id, &mailbox) {
                (Some(a), Some(m)) => stmt
                    .query_map(params![a, m, limit, offset], row_to_summary)?
                    .collect(),
                (Some(a), None) => stmt
                    .query_map(params![a, limit, offset], row_to_summary)?
                    .collect(),
                (None, Some(m)) => stmt
                    .query_map(params![m, limit, offset], row_to_summary)?
                    .collect(),
                (None, None) => stmt
                    .query_map(params![limit, offset], row_to_summary)?
                    .collect(),
            };
            Ok(rows?)
        })
        .await
    }

    pub async fn get_message(&self, id: i64) -> Result<Option<Message>> {
        self.run(move |conn| {
            let mut stmt = conn.prepare("SELECT * FROM messages WHERE id = ?1")?;
            Ok(stmt.query_row(params![id], row_to_message).optional()?)
        })
        .await
    }

    /// On-disk path of the raw .eml backing a `fetched_emails` row.
    pub async fn raw_email_path(&self, fetched_email_id: i64) -> Result<Option<String>> {
        self.run(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT file_path FROM fetched_emails WHERE id = ?1",
                    params![fetched_email_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?)
        })
        .await
    }

    /// New fetched emails that don't yet have a `messages` row (e.g. parser
    /// hasn't run, or the row was deleted manually). Used by the parser
    /// backfill pass on startup.
    pub async fn unparsed_fetches(&self, limit: i64) -> Result<Vec<PendingFetch>> {
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT fe.id, fe.account_id, fe.mailbox, fe.file_path
                 FROM fetched_emails fe
                 LEFT JOIN messages m ON m.fetched_email_id = fe.id
                 WHERE m.id IS NULL
                 LIMIT ?1",
            )?;
            let rows: Result<Vec<PendingFetch>, _> = stmt
                .query_map(params![limit], |row| {
                    Ok(PendingFetch {
                        fetched_email_id: row.get(0)?,
                        account_id: row.get(1)?,
                        mailbox: row.get(2)?,
                        file_path: row.get(3)?,
                    })
                })?
                .collect();
            Ok(rows?)
        })
        .await
    }
}
