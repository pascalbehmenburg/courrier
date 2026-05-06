//! Per-account (and global) fetch run tracking.

use super::Database;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FetchRunStatus {
    Running,
    Completed,
    Failed,
}

impl FetchRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            FetchRunStatus::Running => "running",
            FetchRunStatus::Completed => "completed",
            FetchRunStatus::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "running" => FetchRunStatus::Running,
            "completed" => FetchRunStatus::Completed,
            _ => FetchRunStatus::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FetchRun {
    pub id: i64,
    pub account_id: Option<i64>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub messages_fetched: i64,
    pub status: FetchRunStatus,
    pub error: Option<String>,
}

fn parse(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn row_to_run(row: &Row<'_>) -> rusqlite::Result<FetchRun> {
    Ok(FetchRun {
        id: row.get("id")?,
        account_id: row.get("account_id")?,
        started_at: parse(row.get("started_at")?),
        completed_at: row.get::<_, Option<String>>("completed_at")?.map(parse),
        messages_fetched: row.get("messages_fetched")?,
        status: FetchRunStatus::parse(&row.get::<_, String>("status")?),
        error: row.get("error")?,
    })
}

impl Database {
    pub async fn start_fetch_run(&self, account_id: Option<i64>) -> Result<i64> {
        self.run(move |conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO fetch_runs (account_id, started_at, status)
                 VALUES (?1, ?2, 'running')",
                params![account_id, now],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
    }

    pub async fn record_fetch_run_progress(&self, run_id: i64, additional: i64) -> Result<()> {
        self.run(move |conn| {
            conn.execute(
                "UPDATE fetch_runs
                 SET messages_fetched = messages_fetched + ?1
                 WHERE id = ?2",
                params![additional, run_id],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn complete_fetch_run(
        &self,
        run_id: i64,
        status: FetchRunStatus,
        error: Option<String>,
    ) -> Result<()> {
        self.run(move |conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE fetch_runs
                 SET completed_at = ?1, status = ?2, error = ?3
                 WHERE id = ?4",
                params![now, status.as_str(), error, run_id],
            )?;
            Ok(())
        })
        .await
    }

    /// Latest run for a given account (or for the global "all accounts"
    /// trigger when `account_id` is None).
    pub async fn latest_fetch_run(&self, account_id: Option<i64>) -> Result<Option<FetchRun>> {
        self.run(move |conn| {
            let mut stmt = match account_id {
                Some(_) => conn.prepare(
                    "SELECT id, account_id, started_at, completed_at, messages_fetched, status, error
                     FROM fetch_runs WHERE account_id = ?1
                     ORDER BY started_at DESC LIMIT 1",
                )?,
                None => conn.prepare(
                    "SELECT id, account_id, started_at, completed_at, messages_fetched, status, error
                     FROM fetch_runs WHERE account_id IS NULL
                     ORDER BY started_at DESC LIMIT 1",
                )?,
            };
            Ok(match account_id {
                Some(id) => stmt.query_row(params![id], row_to_run).optional()?,
                None => stmt.query_row([], row_to_run).optional()?,
            })
        })
        .await
    }

    /// One row per account = its latest fetch_run. Useful for the dashboard
    /// "per-account status" panel.
    pub async fn latest_run_per_account(&self) -> Result<Vec<FetchRun>> {
        self.run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT r.id, r.account_id, r.started_at, r.completed_at,
                        r.messages_fetched, r.status, r.error
                 FROM fetch_runs r
                 INNER JOIN (
                    SELECT account_id, MAX(started_at) AS latest
                    FROM fetch_runs
                    WHERE account_id IS NOT NULL
                    GROUP BY account_id
                 ) latest_runs
                 ON r.account_id = latest_runs.account_id
                 AND r.started_at = latest_runs.latest",
            )?;
            let rows: Result<Vec<FetchRun>, _> = stmt.query_map([], row_to_run)?.collect();
            Ok(rows?)
        })
        .await
    }
}
