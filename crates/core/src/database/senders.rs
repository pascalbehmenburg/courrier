//! Sender tracking: one row per distinct From: address we've ever
//! parsed. Powers the bulk-unsubscribe view.

use super::Database;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Row};
use serde::Serialize;

/// Optimistic upsert payload — caller provides whatever they know; null
/// fields don't clobber existing values.
#[derive(Debug, Clone, Default)]
pub struct SenderObservation {
    pub address: String,
    pub display_name: Option<String>,
    pub seen_at: DateTime<Utc>,
    pub unsub_one_click_url: Option<String>,
    pub unsub_mailto: Option<String>,
    pub unsub_web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Sender {
    pub id: i64,
    pub address: String,
    pub display_name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub message_count: i64,
    pub unsub_one_click_url: Option<String>,
    pub unsub_mailto: Option<String>,
    pub unsub_web_url: Option<String>,
    pub unsubscribed_at: Option<DateTime<Utc>>,
    pub unsubscribed_method: Option<String>,
    pub unsubscribe_result: Option<String>,
}

fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn row_to_sender(row: &Row<'_>) -> rusqlite::Result<Sender> {
    Ok(Sender {
        id: row.get("id")?,
        address: row.get("address")?,
        display_name: row.get("display_name")?,
        first_seen_at: parse_dt(row.get("first_seen_at")?).unwrap_or_else(Utc::now),
        last_seen_at: parse_dt(row.get("last_seen_at")?).unwrap_or_else(Utc::now),
        message_count: row.get("message_count")?,
        unsub_one_click_url: row.get("unsub_one_click_url")?,
        unsub_mailto: row.get("unsub_mailto")?,
        unsub_web_url: row.get("unsub_web_url")?,
        unsubscribed_at: parse_dt(row.get("unsubscribed_at")?),
        unsubscribed_method: row.get("unsubscribed_method")?,
        unsubscribe_result: row.get("unsubscribe_result")?,
    })
}

impl Database {
    /// Insert or update a sender row from a single observation. Bumps
    /// last_seen_at + message_count; only fills unsubscribe URLs if not
    /// already set (we keep the first observed URL — they tend to be
    /// stable per-subscription).
    pub async fn upsert_sender(&self, obs: SenderObservation) -> Result<i64> {
        self.run(move |conn| {
            let SenderObservation {
                address,
                display_name,
                seen_at,
                unsub_one_click_url,
                unsub_mailto,
                unsub_web_url,
            } = obs;
            let address = address.to_ascii_lowercase();
            let seen_str = seen_at.to_rfc3339();
            conn.execute(
                "INSERT INTO senders (
                    address, display_name,
                    first_seen_at, last_seen_at, message_count,
                    unsub_one_click_url, unsub_mailto, unsub_web_url
                ) VALUES (?1, ?2, ?3, ?3, 1, ?4, ?5, ?6)
                ON CONFLICT(address) DO UPDATE SET
                    display_name = COALESCE(display_name, excluded.display_name),
                    last_seen_at = CASE
                        WHEN excluded.last_seen_at > last_seen_at THEN excluded.last_seen_at
                        ELSE last_seen_at
                    END,
                    first_seen_at = CASE
                        WHEN excluded.last_seen_at < first_seen_at THEN excluded.last_seen_at
                        ELSE first_seen_at
                    END,
                    message_count = message_count + 1,
                    unsub_one_click_url = COALESCE(unsub_one_click_url, excluded.unsub_one_click_url),
                    unsub_mailto = COALESCE(unsub_mailto, excluded.unsub_mailto),
                    unsub_web_url = COALESCE(unsub_web_url, excluded.unsub_web_url)",
                params![
                    address,
                    display_name,
                    seen_str,
                    unsub_one_click_url,
                    unsub_mailto,
                    unsub_web_url,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
    }

    /// List senders. `kind` selects subscriptions vs all; `since` filters
    /// to `last_seen_at >= since` (the 6-month-active window).
    pub async fn list_senders(
        &self,
        kind: SenderKind,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Sender>> {
        self.run(move |conn| {
            let mut clauses: Vec<&'static str> = Vec::new();
            // The "subscribed" predicate is:
            //   unsubscribed_at IS NULL OR last_seen_at > unsubscribed_at
            // i.e. a new mail after our recorded unsubscribe re-flags it.
            match kind {
                SenderKind::OneClickSubscribed => {
                    clauses.push("unsub_one_click_url IS NOT NULL");
                    clauses.push("(unsubscribed_at IS NULL OR last_seen_at > unsubscribed_at)");
                }
                SenderKind::ManualSubscribed => {
                    clauses.push("unsub_one_click_url IS NULL");
                    clauses.push("(unsub_mailto IS NOT NULL OR unsub_web_url IS NOT NULL)");
                    clauses.push("(unsubscribed_at IS NULL OR last_seen_at > unsubscribed_at)");
                }
                SenderKind::OtherSubscribed => {
                    // No unsubscribe header at all but mail keeps coming in.
                    // Defaulted to "still subscribed" per the user's rule.
                    clauses.push("unsub_one_click_url IS NULL");
                    clauses.push("unsub_mailto IS NULL");
                    clauses.push("unsub_web_url IS NULL");
                    clauses.push("(unsubscribed_at IS NULL OR last_seen_at > unsubscribed_at)");
                }
                SenderKind::Unsubscribed => {
                    clauses.push("unsubscribed_at IS NOT NULL");
                    clauses.push("last_seen_at <= unsubscribed_at");
                }
                SenderKind::All => {}
            }
            let since_str = since.map(|s| s.to_rfc3339());
            if since_str.is_some() {
                clauses.push("last_seen_at >= ?1");
            }
            let where_sql = if clauses.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", clauses.join(" AND "))
            };
            let sql = format!(
                "SELECT id, address, display_name, first_seen_at, last_seen_at,
                        message_count, unsub_one_click_url, unsub_mailto, unsub_web_url,
                        unsubscribed_at, unsubscribed_method, unsubscribe_result
                 FROM senders{where_sql}
                 ORDER BY message_count DESC, last_seen_at DESC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows: Result<Vec<Sender>, _> = match since_str {
                Some(s) => stmt.query_map(params![s], row_to_sender)?.collect(),
                None => stmt.query_map([], row_to_sender)?.collect(),
            };
            Ok(rows?)
        })
        .await
    }

    pub async fn get_sender(&self, id: i64) -> Result<Option<Sender>> {
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, address, display_name, first_seen_at, last_seen_at,
                        message_count, unsub_one_click_url, unsub_mailto, unsub_web_url,
                        unsubscribed_at, unsubscribed_method, unsubscribe_result
                 FROM senders WHERE id = ?1",
            )?;
            let row: Option<Sender> = stmt.query_row(params![id], row_to_sender).ok();
            Ok(row)
        })
        .await
    }

    /// Record the result of an unsubscribe attempt. `method` is free-form
    /// (`one_click`, `manual_link`, `mailto`, `skip`).
    pub async fn mark_unsubscribed(
        &self,
        id: i64,
        method: &str,
        result: Option<&str>,
    ) -> Result<()> {
        let method = method.to_string();
        let result = result.map(|s| s.to_string());
        self.run(move |conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE senders
                 SET unsubscribed_at = ?2,
                     unsubscribed_method = ?3,
                     unsubscribe_result = ?4
                 WHERE id = ?1",
                params![id, now, method, result],
            )?;
            Ok(())
        })
        .await
    }

    /// Record a *failed* unsubscribe attempt without clearing or setting
    /// `unsubscribed_at`. The user can retry, or escalate to manual.
    pub async fn record_unsubscribe_attempt(
        &self,
        id: i64,
        method: &str,
        result: &str,
    ) -> Result<()> {
        let method = method.to_string();
        let result = result.to_string();
        self.run(move |conn| {
            conn.execute(
                "UPDATE senders
                 SET unsubscribed_method = ?2,
                     unsubscribe_result = ?3
                 WHERE id = ?1",
                params![id, method, result],
            )?;
            Ok(())
        })
        .await
    }

    /// Re-subscribe: clear unsubscribed_at. Useful for testing and for
    /// the case where a sender starts sending again after we'd opted out
    /// (the automatic re-flag already handles "subscribed" derivation,
    /// but a user may want to explicitly clear the record too).
    pub async fn resubscribe(&self, id: i64) -> Result<()> {
        self.run(move |conn| {
            conn.execute(
                "UPDATE senders
                 SET unsubscribed_at = NULL,
                     unsubscribed_method = NULL
                 WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
        .await
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SenderKind {
    OneClickSubscribed,
    ManualSubscribed,
    OtherSubscribed,
    Unsubscribed,
    All,
}
