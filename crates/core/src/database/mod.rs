//! Async-friendly SQLite handle. All public methods are async and route
//! through `run()`, which moves the actual sqlite work to a blocking
//! thread so tokio runtime workers stay free.

mod accounts;
mod fetch_runs;
mod messages;
mod schema;
mod senders;

pub use accounts::{Account, AccountInput};
pub use fetch_runs::{FetchRun, FetchRunStatus};
pub use messages::{Message, MessageRow, MessageSummary};
pub use senders::{Sender, SenderKind, SenderObservation};

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        // Enable foreign keys (off by default per SQLite) so ON DELETE
        // CASCADE between accounts → fetched_emails → messages works.
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Database {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock();
        for stmt in schema::SCHEMA {
            conn.execute(stmt, [])?;
        }
        // Idempotent column adds for pre-existing DBs. ADD COLUMN errors if
        // the column already exists, so probe first.
        let has_col = |table: &str, col: &str| -> Result<bool> {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
            let cols: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(cols.iter().any(|c| c == col))
        };
        if !has_col("messages", "original_sender_addr")? {
            conn.execute(
                "ALTER TABLE messages ADD COLUMN original_sender_addr TEXT",
                [],
            )?;
        }
        Ok(())
    }

    /// Run a closure with the SQLite connection on a blocking thread.
    /// Closures own their inputs (clones of caller-supplied data); this is
    /// the price of the 'static bound that spawn_blocking requires.
    pub(crate) async fn run<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();
            f(&conn)
        })
        .await?
    }
}
