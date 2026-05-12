//! Account CRUD. Passwords are never stored or returned in plaintext —
//! callers hand us ciphertext (already encrypted by the Encryptor) and
//! receive ciphertext back.

use super::Database;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub label: String,
    pub email: String,
    pub username: String,
    pub host: String,
    pub port: u16,
    pub provider_id: String,
    pub sync_interval_seconds: Option<u64>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// AES-GCM ciphertext (base64). Never serialized over the API.
    #[serde(skip)]
    pub password_ciphertext: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountInput {
    pub label: String,
    pub email: String,
    pub username: String,
    pub host: String,
    pub port: u16,
    pub provider_id: String,
    pub sync_interval_seconds: Option<u64>,
    pub enabled: bool,
    /// Already-encrypted password (caller has access to the Encryptor).
    pub password_ciphertext: String,
}

fn row_to_account(row: &Row<'_>) -> rusqlite::Result<Account> {
    let parse = |s: String| -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    };
    Ok(Account {
        id: row.get("id")?,
        label: row.get("label")?,
        email: row.get("email")?,
        username: row.get("username")?,
        host: row.get("host")?,
        port: row.get::<_, i64>("port")? as u16,
        provider_id: row.get("provider_id")?,
        sync_interval_seconds: row
            .get::<_, Option<i64>>("sync_interval_seconds")?
            .map(|v| v as u64),
        enabled: row.get::<_, i64>("enabled")? != 0,
        created_at: parse(row.get("created_at")?),
        updated_at: parse(row.get("updated_at")?),
        password_ciphertext: row.get("password_ciphertext")?,
    })
}

impl Database {
    pub async fn list_accounts(&self) -> Result<Vec<Account>> {
        self.run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, label, email, username, password_ciphertext, host, port,
                        provider_id, sync_interval_seconds, enabled, created_at, updated_at
                 FROM accounts ORDER BY label",
            )?;
            let accounts: Result<Vec<Account>, _> = stmt.query_map([], row_to_account)?.collect();
            Ok(accounts?)
        })
        .await
    }

    pub async fn get_account(&self, id: i64) -> Result<Option<Account>> {
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, label, email, username, password_ciphertext, host, port,
                        provider_id, sync_interval_seconds, enabled, created_at, updated_at
                 FROM accounts WHERE id = ?1",
            )?;
            Ok(stmt.query_row(params![id], row_to_account).optional()?)
        })
        .await
    }

    pub async fn insert_account(&self, input: AccountInput) -> Result<Account> {
        self.run(move |conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO accounts
                    (label, email, username, password_ciphertext, host, port,
                     provider_id, sync_interval_seconds, enabled, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                params![
                    input.label,
                    input.email,
                    input.username,
                    input.password_ciphertext,
                    input.host,
                    input.port,
                    input.provider_id,
                    input.sync_interval_seconds.map(|v| v as i64),
                    input.enabled as i64,
                    now,
                ],
            )?;
            let id = conn.last_insert_rowid();
            let mut stmt = conn.prepare(
                "SELECT id, label, email, username, password_ciphertext, host, port,
                        provider_id, sync_interval_seconds, enabled, created_at, updated_at
                 FROM accounts WHERE id = ?1",
            )?;
            Ok(stmt.query_row(params![id], row_to_account)?)
        })
        .await
    }

    pub async fn update_account(&self, id: i64, input: AccountInput) -> Result<Account> {
        self.run(move |conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE accounts SET
                    label = ?2, email = ?3, username = ?4, password_ciphertext = ?5,
                    host = ?6, port = ?7, provider_id = ?8,
                    sync_interval_seconds = ?9, enabled = ?10, updated_at = ?11
                 WHERE id = ?1",
                params![
                    id,
                    input.label,
                    input.email,
                    input.username,
                    input.password_ciphertext,
                    input.host,
                    input.port,
                    input.provider_id,
                    input.sync_interval_seconds.map(|v| v as i64),
                    input.enabled as i64,
                    now,
                ],
            )?;
            let mut stmt = conn.prepare(
                "SELECT id, label, email, username, password_ciphertext, host, port,
                        provider_id, sync_interval_seconds, enabled, created_at, updated_at
                 FROM accounts WHERE id = ?1",
            )?;
            Ok(stmt.query_row(params![id], row_to_account)?)
        })
        .await
    }

    pub async fn delete_account(&self, id: i64) -> Result<bool> {
        self.run(move |conn| {
            let n = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
            Ok(n > 0)
        })
        .await
    }
}
