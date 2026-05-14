//! Bulk-unsubscribe logic.
//!
//! Issues RFC 8058 one-click POSTs to `List-Unsubscribe` URLs and
//! records the outcome on the `senders` row. The HTTP work happens here
//! so route handlers stay thin and the same primitive can be used from
//! a CLI later if we ever want one.

use crate::database::Database;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsubscribeOutcome {
    pub sender_id: i64,
    pub address: String,
    pub ok: bool,
    /// HTTP status code on success, or a short error label.
    pub status: String,
}

/// Single one-click POST per RFC 8058.
async fn one_click_post(url: &str) -> (bool, String) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("courrier/0.2 unsubscribe-bot")
        .build()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("client init: {e}")),
    };
    let resp = client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body("List-Unsubscribe=One-Click")
        .send()
        .await;
    match resp {
        Ok(r) => {
            let status = r.status();
            // 2xx is success. Some lists return 3xx after the body has
            // already been consumed; reqwest will have followed those.
            (status.is_success(), status.as_u16().to_string())
        }
        Err(e) => {
            let label = if e.is_timeout() {
                "timeout".to_string()
            } else if e.is_connect() {
                "connect".to_string()
            } else {
                format!("error: {e}")
            };
            (false, label)
        }
    }
}

/// Try to unsubscribe every supplied sender via one-click. Concurrent
/// up to `max_concurrent`. Persists the result onto each sender row.
pub async fn bulk_one_click(
    db: &Database,
    sender_ids: &[i64],
    max_concurrent: usize,
) -> Result<Vec<UnsubscribeOutcome>> {
    use futures::stream::{FuturesUnordered, StreamExt};

    // Load senders + filter to those with a one-click URL.
    let mut targets: Vec<(i64, String, String)> = Vec::new();
    for id in sender_ids {
        if let Some(s) = db.get_sender(*id).await? {
            if let Some(url) = s.unsub_one_click_url {
                targets.push((s.id, s.address, url));
            }
        }
    }

    let mut in_flight = FuturesUnordered::new();
    let mut outcomes: Vec<UnsubscribeOutcome> = Vec::with_capacity(targets.len());
    let mut iter = targets.into_iter();

    // Prime the pump.
    for _ in 0..max_concurrent {
        let Some((id, addr, url)) = iter.next() else {
            break;
        };
        in_flight.push(run_one(id, addr, url));
    }

    while let Some(outcome) = in_flight.next().await {
        // Persist the result before queueing the next so a crash leaves
        // the DB consistent with what's already been hit.
        let method = "one_click";
        let result_label = if outcome.ok {
            format!("HTTP {}", outcome.status)
        } else {
            outcome.status.clone()
        };
        if outcome.ok {
            db.mark_unsubscribed(outcome.sender_id, method, Some(&result_label))
                .await?;
        } else {
            // Still record the attempt so the UI can show "tried, failed".
            // Don't set unsubscribed_at — they're still considered subscribed.
            db.record_unsubscribe_attempt(outcome.sender_id, method, &result_label)
                .await?;
        }
        outcomes.push(outcome);
        if let Some((id, addr, url)) = iter.next() {
            in_flight.push(run_one(id, addr, url));
        }
    }

    Ok(outcomes)
}

async fn run_one(sender_id: i64, address: String, url: String) -> UnsubscribeOutcome {
    let (ok, status) = one_click_post(&url).await;
    UnsubscribeOutcome {
        sender_id,
        address,
        ok,
        status,
    }
}
