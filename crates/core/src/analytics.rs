//! Aggregate views over the `messages` table powering the dashboard.
//!
//! All queries scope to a single account when an id is supplied, or report
//! across the whole DB when it isn't.

use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use crate::database::Database;

#[derive(Debug, Clone, Serialize)]
pub struct OverviewStats {
    pub account_id: Option<i64>,
    pub total_messages: i64,
    pub total_storage_bytes: i64,
    pub forwarded_messages: i64,
    pub mailbox_count: i64,
    pub last_message_date: Option<String>,
    pub first_message_date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CountedString {
    pub key: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DateBucket {
    pub day: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForwardingBreakdown {
    /// e.g. {"forwarded_from": "ambien@web.de", "count": 5000}
    pub by_forwarder: Vec<CountedString>,
    /// Among messages forwarded from a specific address, the original-sender
    /// domains. Key is "<forwarder> -> <origin domain>".
    pub by_forwarder_then_origin: Vec<ForwarderOriginRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForwarderOriginRow {
    pub forwarded_from: String,
    pub origin_domain: String,
    pub count: i64,
}

impl Database {
    pub async fn overview_stats(&self, account_id: Option<i64>) -> Result<OverviewStats> {
        self.run(move |conn| {
            let scope = if account_id.is_some() {
                " WHERE account_id = ?1"
            } else {
                ""
            };
            let sql = format!(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(size_bytes), 0),
                    SUM(CASE WHEN is_forwarded = 1 THEN 1 ELSE 0 END),
                    COUNT(DISTINCT mailbox),
                    MAX(date_utc),
                    MIN(date_utc)
                 FROM messages{scope}",
            );
            let mut stmt = conn.prepare(&sql)?;
            let row = match account_id {
                Some(a) => stmt.query_row(params![a], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })?,
                None => stmt.query_row([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })?,
            };
            Ok(OverviewStats {
                account_id,
                total_messages: row.0,
                total_storage_bytes: row.1,
                forwarded_messages: row.2,
                mailbox_count: row.3,
                last_message_date: row.4,
                first_message_date: row.5,
            })
        })
        .await
    }

    pub async fn top_senders(
        &self,
        account_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<CountedString>> {
        self.counted(
            "from_addr",
            "from_addr IS NOT NULL AND from_addr != ''",
            account_id,
            limit,
        )
        .await
    }

    pub async fn top_sender_domains(
        &self,
        account_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<CountedString>> {
        self.run(move |conn| {
            let scope = if account_id.is_some() {
                " AND account_id = ?1"
            } else {
                ""
            };
            let sql = format!(
                "SELECT LOWER(SUBSTR(from_addr, INSTR(from_addr, '@') + 1)) AS domain,
                        COUNT(*) AS c
                 FROM messages
                 WHERE from_addr IS NOT NULL AND INSTR(from_addr, '@') > 0{scope}
                 GROUP BY domain ORDER BY c DESC LIMIT ?",
            );
            let mut stmt = conn.prepare(&sql)?;
            let map = |row: &rusqlite::Row<'_>| {
                Ok(CountedString {
                    key: row.get(0)?,
                    count: row.get(1)?,
                })
            };
            let rows: Result<Vec<_>, _> = match account_id {
                Some(a) => stmt.query_map(params![a, limit], map)?.collect(),
                None => stmt.query_map(params![limit], map)?.collect(),
            };
            Ok(rows?)
        })
        .await
    }

    pub async fn forwarding_breakdown(
        &self,
        account_id: Option<i64>,
        limit: i64,
    ) -> Result<ForwardingBreakdown> {
        let by_forwarder = self
            .counted(
                "forwarded_from",
                "is_forwarded = 1 AND forwarded_from IS NOT NULL",
                account_id,
                limit,
            )
            .await?;

        let by_forwarder_then_origin = self
            .run(move |conn| {
                let scope = if account_id.is_some() {
                    " AND account_id = ?1"
                } else {
                    ""
                };
                let sql = format!(
                    "SELECT forwarded_from, original_sender_domain, COUNT(*) AS c
                     FROM messages
                     WHERE is_forwarded = 1
                       AND forwarded_from IS NOT NULL
                       AND original_sender_domain IS NOT NULL{scope}
                     GROUP BY forwarded_from, original_sender_domain
                     ORDER BY c DESC LIMIT ?",
                );
                let mut stmt = conn.prepare(&sql)?;
                let map = |row: &rusqlite::Row<'_>| {
                    Ok(ForwarderOriginRow {
                        forwarded_from: row.get(0)?,
                        origin_domain: row.get(1)?,
                        count: row.get(2)?,
                    })
                };
                let rows: Result<Vec<_>, _> = match account_id {
                    Some(a) => stmt.query_map(params![a, limit], map)?.collect(),
                    None => stmt.query_map(params![limit], map)?.collect(),
                };
                Ok(rows?)
            })
            .await?;

        Ok(ForwardingBreakdown {
            by_forwarder,
            by_forwarder_then_origin,
        })
    }

    pub async fn messages_per_day(
        &self,
        account_id: Option<i64>,
        days: i64,
    ) -> Result<Vec<DateBucket>> {
        self.run(move |conn| {
            let scope = if account_id.is_some() {
                " AND account_id = ?1"
            } else {
                ""
            };
            let sql = format!(
                "SELECT SUBSTR(date_utc, 1, 10) AS day, COUNT(*) AS c
                 FROM messages
                 WHERE date_utc IS NOT NULL AND date_utc >= datetime('now', '-' || ? || ' days'){scope}
                 GROUP BY day ORDER BY day",
            );
            let mut stmt = conn.prepare(&sql)?;
            let map = |row: &rusqlite::Row<'_>| {
                Ok(DateBucket {
                    day: row.get(0)?,
                    count: row.get(1)?,
                })
            };
            let rows: Result<Vec<_>, _> = match account_id {
                Some(a) => stmt.query_map(params![days, a], map)?.collect(),
                None => stmt.query_map(params![days], map)?.collect(),
            };
            Ok(rows?)
        })
        .await
    }

    pub async fn mailbox_distribution(
        &self,
        account_id: Option<i64>,
    ) -> Result<Vec<CountedString>> {
        self.counted("mailbox", "1=1", account_id, 100).await
    }

    async fn counted(
        &self,
        column: &'static str,
        predicate: &'static str,
        account_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<CountedString>> {
        self.run(move |conn| {
            let scope = if account_id.is_some() {
                " AND account_id = ?1"
            } else {
                ""
            };
            let sql = format!(
                "SELECT {column} AS key, COUNT(*) AS c
                 FROM messages WHERE {predicate}{scope}
                 GROUP BY {column} ORDER BY c DESC LIMIT ?",
            );
            let mut stmt = conn.prepare(&sql)?;
            let map = |row: &rusqlite::Row<'_>| {
                Ok(CountedString {
                    key: row.get(0)?,
                    count: row.get(1)?,
                })
            };
            let rows: Result<Vec<_>, _> = match account_id {
                Some(a) => stmt.query_map(params![a, limit], map)?.collect(),
                None => stmt.query_map(params![limit], map)?.collect(),
            };
            Ok(rows?)
        })
        .await
    }
}
