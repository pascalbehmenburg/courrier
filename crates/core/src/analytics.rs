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

/// Nested forwarder → domain → address breakdown for the UI drill-down.
#[derive(Debug, Clone, Serialize)]
pub struct ForwarderTree {
    pub forwarders: Vec<ForwarderNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForwarderNode {
    /// `forwarded_from` if known, otherwise the forwarder *domain*
    /// (envelope-only SRS hits with no matching To: address).
    pub forwarder: String,
    pub total: i64,
    pub domains: Vec<OriginDomainNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OriginDomainNode {
    pub domain: String,
    pub count: i64,
    pub addresses: Vec<CountedString>,
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

    /// Nested forwarder → domain → address breakdown. Caps at
    /// (max_forwarders, max_domains_per_fwd, max_addrs_per_domain) to
    /// keep the payload bounded. The forwarder key is `forwarded_from`
    /// if set, falling back to `forwarded_from_domain`.
    pub async fn forwarder_tree(
        &self,
        account_id: Option<i64>,
        max_forwarders: i64,
        max_domains_per_forwarder: i64,
        max_addrs_per_domain: i64,
    ) -> Result<ForwarderTree> {
        self.run(move |conn| {
            let scope = if account_id.is_some() {
                " AND account_id = ?1"
            } else {
                ""
            };
            // One pass: pull (forwarder_key, domain, address, count) for
            // every (forwarder, original_sender_domain, original_sender_addr)
            // tuple. Domain falls back to substring of address when only
            // address is present and vice-versa.
            let sql = format!(
                "SELECT
                    COALESCE(forwarded_from, forwarded_from_domain) AS forwarder,
                    COALESCE(
                        original_sender_domain,
                        LOWER(SUBSTR(original_sender_addr, INSTR(original_sender_addr, '@') + 1))
                    ) AS domain,
                    LOWER(original_sender_addr) AS addr,
                    COUNT(*) AS c
                 FROM messages
                 WHERE is_forwarded = 1
                   AND (forwarded_from IS NOT NULL OR forwarded_from_domain IS NOT NULL){scope}
                 GROUP BY forwarder, domain, addr",
            );
            let mut stmt = conn.prepare(&sql)?;
            type Triple = (Option<String>, Option<String>, Option<String>, i64);
            let map = |row: &rusqlite::Row<'_>| -> rusqlite::Result<Triple> {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            };
            let rows: Vec<Triple> = match account_id {
                Some(a) => stmt
                    .query_map(params![a], map)?
                    .collect::<Result<Vec<_>, _>>()?,
                None => stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?,
            };

            // forwarder -> domain -> addr -> count
            use std::collections::BTreeMap;
            let mut tree: BTreeMap<String, BTreeMap<String, BTreeMap<Option<String>, i64>>> =
                BTreeMap::new();
            for (fwd, dom, addr, c) in rows {
                let Some(fwd) = fwd else { continue };
                let dom = dom.unwrap_or_else(|| "(unknown)".to_string());
                *tree
                    .entry(fwd)
                    .or_default()
                    .entry(dom)
                    .or_default()
                    .entry(addr)
                    .or_insert(0) += c;
            }

            // Materialise + sort + cap.
            let mut forwarders: Vec<ForwarderNode> = tree
                .into_iter()
                .map(|(forwarder, dom_map)| {
                    let mut domains: Vec<OriginDomainNode> = dom_map
                        .into_iter()
                        .map(|(domain, addr_map)| {
                            let domain_total: i64 = addr_map.values().sum();
                            let mut addresses: Vec<CountedString> = addr_map
                                .into_iter()
                                .filter_map(|(addr, count)| {
                                    addr.map(|key| CountedString { key, count })
                                })
                                .collect();
                            addresses.sort_by(|a, b| b.count.cmp(&a.count));
                            addresses.truncate(max_addrs_per_domain as usize);
                            OriginDomainNode {
                                domain,
                                count: domain_total,
                                addresses,
                            }
                        })
                        .collect();
                    domains.sort_by(|a, b| b.count.cmp(&a.count));
                    let total: i64 = domains.iter().map(|d| d.count).sum();
                    domains.truncate(max_domains_per_forwarder as usize);
                    ForwarderNode {
                        forwarder,
                        total,
                        domains,
                    }
                })
                .collect();
            forwarders.sort_by(|a, b| b.total.cmp(&a.total));
            forwarders.truncate(max_forwarders as usize);

            Ok(ForwarderTree { forwarders })
        })
        .await
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
