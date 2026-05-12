//! Full-text search backed by the SQLite FTS5 virtual table set up in
//! `database/schema.rs`.

use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use crate::database::Database;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub id: i64,
    pub account_id: i64,
    pub mailbox: String,
    pub subject: Option<String>,
    pub from_addr: Option<String>,
    pub from_name: Option<String>,
    pub date_utc: Option<String>,
    pub snippet: String,
    pub rank: f64,
}

impl Database {
    /// Run an FTS5 query. The query string is passed straight to FTS5, so
    /// callers can use its query syntax (`"phrase"`, `term1 AND term2`,
    /// `column:value`, …). See the SQLite FTS5 docs for the full grammar.
    pub async fn search_messages(
        &self,
        query: String,
        account_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<SearchHit>> {
        self.run(move |conn| {
            // snippet(table, col, "<b>", "</b>", "…", tokens)
            let mut sql = String::from(
                "SELECT m.id, m.account_id, m.mailbox, m.subject, m.from_addr, m.from_name,
                        m.date_utc,
                        snippet(messages_fts, 4, '<mark>', '</mark>', '…', 12) AS snippet,
                        bm25(messages_fts) AS rank
                 FROM messages_fts
                 JOIN messages m ON m.id = messages_fts.rowid
                 WHERE messages_fts MATCH ?1",
            );
            if account_id.is_some() {
                sql.push_str(" AND m.account_id = ?2");
            }
            sql.push_str(" ORDER BY rank LIMIT ?");
            let mut stmt = conn.prepare(&sql)?;

            let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<SearchHit> {
                Ok(SearchHit {
                    id: row.get("id")?,
                    account_id: row.get("account_id")?,
                    mailbox: row.get("mailbox")?,
                    subject: row.get("subject")?,
                    from_addr: row.get("from_addr")?,
                    from_name: row.get("from_name")?,
                    date_utc: row.get("date_utc")?,
                    snippet: row.get("snippet")?,
                    rank: row.get("rank")?,
                })
            };

            let hits: Result<Vec<_>, _> = match account_id {
                Some(a) => stmt.query_map(params![query, a, limit], map_row)?.collect(),
                None => stmt.query_map(params![query, limit], map_row)?.collect(),
            };
            Ok(hits?)
        })
        .await
    }
}
