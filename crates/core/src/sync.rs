//! High-level sync coordinator.
//!
//! Holds per-account in-flight `JoinHandle`s so the API and the scheduler
//! can both ask "is account X currently syncing?" and refuse to spawn a
//! duplicate. Owns the periodic sweeper that wakes every minute and
//! triggers any account whose `sync_interval_seconds` has elapsed.

use anyhow::Result;
use chrono::Utc;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::database::Database;
use crate::encryption::Encryptor;
use crate::fetcher::{self, AccountSecrets};

#[derive(Clone)]
pub struct SyncCoordinator {
    db: Database,
    encryptor: Encryptor,
    storage_path: Arc<PathBuf>,
    in_flight: Arc<Mutex<HashMap<i64, JoinHandle<Result<fetcher::AccountFetchOutcome>>>>>,
}

impl SyncCoordinator {
    pub fn new(db: Database, encryptor: Encryptor, storage_path: PathBuf) -> Self {
        Self {
            db,
            encryptor,
            storage_path: Arc::new(storage_path),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Trigger a sync for one account. Returns true if a new task was
    /// spawned, false if one was already running.
    pub async fn trigger_one(&self, account_id: i64) -> Result<bool> {
        self.reap_finished();
        if self.in_flight.lock().contains_key(&account_id) {
            return Ok(false);
        }
        let account = self
            .db
            .get_account(account_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("account {} not found", account_id))?;
        let password = self.encryptor.decrypt(&account.password_ciphertext)?;
        let secrets = AccountSecrets {
            username: account.username.clone(),
            password,
        };

        let db = self.db.clone();
        let storage = (*self.storage_path).clone();
        let handle =
            tokio::spawn(
                async move { fetcher::fetch_account(&account, &secrets, &storage, &db).await },
            );

        self.in_flight.lock().insert(account_id, handle);
        Ok(true)
    }

    /// Trigger all enabled accounts that aren't already syncing.
    /// Returns the list of account ids that were started.
    pub async fn trigger_all(&self) -> Result<Vec<i64>> {
        self.reap_finished();
        let accounts = self.db.list_accounts().await?;
        let mut started = Vec::new();
        for acct in accounts {
            if !acct.enabled {
                continue;
            }
            if self.trigger_one(acct.id).await? {
                started.push(acct.id);
            }
        }
        Ok(started)
    }

    /// Account ids whose JoinHandle is still present + not finished.
    pub fn in_flight_account_ids(&self) -> Vec<i64> {
        self.reap_finished();
        self.in_flight.lock().keys().copied().collect()
    }

    pub fn is_running(&self, account_id: i64) -> bool {
        self.reap_finished();
        self.in_flight.lock().contains_key(&account_id)
    }

    /// Clean out completed handles. Called on every public method to keep
    /// the set tight without requiring a background reaper.
    fn reap_finished(&self) {
        let mut guard = self.in_flight.lock();
        guard.retain(|_, handle| !handle.is_finished());
    }

    /// Spawn the periodic scheduler. Wakes every minute and triggers any
    /// account whose configured `sync_interval_seconds` has elapsed since
    /// its last completed run.
    pub fn spawn_scheduler(self: &Arc<Self>) {
        let coord = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
            // Skip the first immediate tick.
            tick.tick().await;
            loop {
                tick.tick().await;
                if let Err(e) = coord.scheduler_pass().await {
                    error!("Scheduler pass failed: {:?}", e);
                }
            }
        });
        info!("Sync scheduler started (1-minute granularity)");
    }

    async fn scheduler_pass(&self) -> Result<()> {
        let now = Utc::now();
        let accounts = self.db.list_accounts().await?;
        for acct in accounts {
            let Some(interval) = acct.sync_interval_seconds else {
                continue;
            };
            if !acct.enabled {
                continue;
            }
            let last = self.db.latest_fetch_run(Some(acct.id)).await?;
            let due = match last.as_ref() {
                None => true,
                Some(run) => match run.completed_at {
                    Some(completed) => (now - completed).num_seconds() as u64 >= interval,
                    None => false, // still running
                },
            };
            if due {
                if let Err(e) = self.trigger_one(acct.id).await {
                    warn!(account_id = acct.id, "scheduler trigger failed: {:?}", e);
                }
            }
        }
        Ok(())
    }

    /// Backfill: parse any .eml on disk that doesn't yet have a `messages`
    /// row. Used at startup so older runs (or DB-only restores) get parsed.
    pub async fn backfill_parser(&self, batch: i64) -> Result<usize> {
        let pending = self.db.unparsed_fetches(batch).await?;
        let total = pending.len();
        for p in pending {
            let Ok(raw) = tokio::fs::read(&p.file_path).await else {
                warn!("Skipping {}: file missing", p.file_path);
                continue;
            };
            match crate::mail::ParsedMail::from_bytes(&raw) {
                Ok(parsed) => {
                    let row = parsed.into_row(p.fetched_email_id, p.account_id, p.mailbox);
                    if let Err(e) = self.db.upsert_message(row).await {
                        warn!("Backfill upsert failed: {:?}", e);
                    }
                }
                Err(e) => warn!(file = %p.file_path, "Backfill parse failed: {:?}", e),
            }
        }
        Ok(total)
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn encryptor(&self) -> &Encryptor {
        &self.encryptor
    }

    pub fn storage_path(&self) -> &PathBuf {
        &self.storage_path
    }
}
