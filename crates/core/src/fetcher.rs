//! IMAP fetch driver.
//!
//! Operates on `Account` rows from the database (passwords decrypted by
//! the caller, never stored back), saves raw .eml files under
//! `<storage_path>/<account>/<mailbox>/<uid>.eml`, records each one in
//! `fetched_emails`, parses the body, and writes the resulting metadata
//! into `messages`. The same path is used by both manual and scheduled
//! syncs.

use anyhow::Result;
use imap::{Client, Session};
use native_tls::{TlsConnector, TlsStream};
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::database::{Account, Database};
use crate::mail::ParsedMail;

const IMAP_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const IMAP_IO_TIMEOUT: Duration = Duration::from_secs(120);

/// Plaintext credentials handed to the fetcher. Never persisted; lives only
/// for the duration of one fetch.
#[derive(Debug, Clone)]
pub struct AccountSecrets {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Default)]
pub struct AccountFetchOutcome {
    pub mailboxes_processed: usize,
    pub messages_saved: usize,
    pub errors: Vec<String>,
}

pub async fn fetch_account(
    account: &Account,
    secrets: &AccountSecrets,
    storage_path: &Path,
    db: &Database,
) -> Result<AccountFetchOutcome> {
    let run_id = db.start_fetch_run(Some(account.id)).await?;
    let result = fetch_account_inner(account, secrets, storage_path, db, run_id).await;
    let (status, error) = match &result {
        Ok(_) => (crate::database::FetchRunStatus::Completed, None),
        Err(e) => (
            crate::database::FetchRunStatus::Failed,
            Some(format!("{e:?}")),
        ),
    };
    if let Err(e) = db.complete_fetch_run(run_id, status, error).await {
        error!("Failed to mark fetch run {} complete: {:?}", run_id, e);
    }
    result
}

async fn fetch_account_inner(
    account: &Account,
    secrets: &AccountSecrets,
    storage_path: &Path,
    db: &Database,
    run_id: i64,
) -> Result<AccountFetchOutcome> {
    info!(account = %account.email, "Syncing account");

    let mailboxes = list_mailboxes(account, secrets).await?;
    info!(account = %account.email, mailboxes = mailboxes.len(), "Discovered mailboxes");

    let mut outcome = AccountFetchOutcome::default();
    for mailbox in &mailboxes {
        match fetch_mailbox(account, secrets, mailbox, storage_path, db).await {
            Ok(count) => {
                outcome.mailboxes_processed += 1;
                outcome.messages_saved += count;
                if let Err(e) = db.record_fetch_run_progress(run_id, count as i64).await {
                    error!("Failed to record fetch progress: {:?}", e);
                }
            }
            Err(e) => {
                outcome.errors.push(format!("{}: {e}", mailbox));
                error!(account = %account.email, mailbox = %mailbox, "Mailbox fetch failed: {:?}", e);
            }
        }
    }

    Ok(outcome)
}

async fn list_mailboxes(account: &Account, secrets: &AccountSecrets) -> Result<Vec<String>> {
    let account = account.clone();
    let secrets = secrets.clone();
    tokio::task::spawn_blocking(move || {
        let mut session = connect_and_login(&account, &secrets)?;
        let mailboxes = session.list(Some(""), Some("*"))?;
        let _ = session.logout();
        Ok::<Vec<String>, anyhow::Error>(mailboxes.iter().map(|n| n.name().to_string()).collect())
    })
    .await?
}

async fn fetch_mailbox(
    account: &Account,
    secrets: &AccountSecrets,
    mailbox: &str,
    storage_path: &Path,
    db: &Database,
) -> Result<usize> {
    let already = db
        .fetched_uids(account.id, mailbox)
        .await?
        .into_iter()
        .collect::<std::collections::HashSet<u32>>();

    let account_clone = account.clone();
    let secrets_clone = secrets.clone();
    let mailbox_str = mailbox.to_string();
    let storage_clone = storage_path.to_path_buf();
    let email_clone = account.email.clone();

    let saved = tokio::task::spawn_blocking(move || {
        let mut session = connect_and_login(&account_clone, &secrets_clone)?;

        debug!("Selecting mailbox: {}", mailbox_str);
        let mailbox_state = match session.select(mailbox_str.as_str()) {
            Ok(m) => m,
            Err(_) => {
                debug!("SELECT failed, trying EXAMINE");
                session.examine(mailbox_str.as_str())?
            }
        };
        info!(mailbox = %mailbox_str, messages = mailbox_state.exists, "Selected mailbox");

        let uids = session.uid_search("NOT DELETED")?;
        let to_fetch: Vec<u32> = uids
            .into_iter()
            .filter(|uid| !already.contains(uid))
            .collect();
        info!(
            mailbox = %mailbox_str,
            already = already.len(),
            new = to_fetch.len(),
            "Mailbox UID survey"
        );

        let mut saved: Vec<(u32, PathBuf, Vec<u8>)> = Vec::new();
        if !to_fetch.is_empty() {
            let account_dir = storage_clone.join(sanitize_path_component(&email_clone));
            let mailbox_dir = account_dir.join(sanitize_path_component(&mailbox_str));
            fs::create_dir_all(&mailbox_dir)?;

            for (idx, uid) in to_fetch.iter().enumerate() {
                debug!("Fetching {}/{} (UID {})", idx + 1, to_fetch.len(), uid);
                match fetch_message_body(&mut session, *uid) {
                    Ok(body) => {
                        let filepath = mailbox_dir.join(format!("{}.eml", uid));
                        if let Err(e) = fs::write(&filepath, &body) {
                            error!("Failed to write {}: {:?}", filepath.display(), e);
                            continue;
                        }
                        saved.push((*uid, filepath, body));
                    }
                    Err(e) => error!("Failed to fetch UID {}: {:?}", uid, e),
                }
            }
        }
        let _ = session.logout();
        Ok::<Vec<(u32, PathBuf, Vec<u8>)>, anyhow::Error>(saved)
    })
    .await??;

    let mut count = 0usize;
    for (uid, filepath, raw) in saved {
        let size = raw.len();
        let fetched_email_id = match db
            .mark_email_fetched(account.id, mailbox, uid, &filepath, size)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                error!("Failed to record UID {}: {:?}", uid, e);
                continue;
            }
        };

        // Parse + persist the message metadata. Failure to parse should not
        // block subsequent messages — log and move on.
        match ParsedMail::from_bytes(&raw) {
            Ok(parsed) => {
                let sender_obs = parsed.sender_observation();
                let row = parsed.into_row(fetched_email_id, account.id, mailbox.to_string());
                if let Err(e) = db.upsert_message(row).await {
                    warn!(uid, "Failed to persist parsed message: {:?}", e);
                }
                if let Some(obs) = sender_obs {
                    if let Err(e) = db.upsert_sender(obs).await {
                        warn!(uid, "Failed to upsert sender: {:?}", e);
                    }
                }
            }
            Err(e) => warn!(uid, "Failed to parse message: {:?}", e),
        }
        count += 1;
    }
    Ok(count)
}

fn fetch_message_body(session: &mut Session<TlsStream<TcpStream>>, uid: u32) -> Result<Vec<u8>> {
    fn try_fetch(
        session: &mut Session<TlsStream<TcpStream>>,
        uid: u32,
        query: &str,
    ) -> Option<Vec<u8>> {
        let msgs = session.uid_fetch(uid.to_string(), query).ok()?;
        let msg = msgs.iter().next()?;
        msg.body().map(Vec::from)
    }
    if let Some(body) = try_fetch(session, uid, "BODY.PEEK[]") {
        return Ok(body);
    }
    try_fetch(session, uid, "RFC822")
        .ok_or_else(|| anyhow::anyhow!("UID {} returned no body for BODY.PEEK[] or RFC822", uid))
}

fn connect_with_timeout(
    server: &str,
    port: u16,
    tls: &TlsConnector,
) -> Result<Client<TlsStream<TcpStream>>> {
    let addr = (server, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("no address resolved for {}:{}", server, port))?;
    let stream = TcpStream::connect_timeout(&addr, IMAP_CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(IMAP_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IMAP_IO_TIMEOUT))?;
    let tls_stream = tls
        .connect(server, stream)
        .map_err(|e| anyhow::anyhow!("TLS handshake failed for {}: {:?}", server, e))?;
    let mut client = Client::new(tls_stream);
    client.read_greeting()?;
    Ok(client)
}

fn try_login(
    server: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<Session<TlsStream<TcpStream>>> {
    let tls = TlsConnector::builder().build()?;
    let client = connect_with_timeout(server, port, &tls)?;
    client
        .login(username, password)
        .map_err(|(e, _)| anyhow::anyhow!("login failed for {}: {:?}", username, e))
}

fn connect_and_login(
    account: &Account,
    secrets: &AccountSecrets,
) -> Result<Session<TlsStream<TcpStream>>> {
    info!(
        host = %account.host,
        port = account.port,
        email = %account.email,
        "Connecting to IMAP server"
    );
    let first_err = match try_login(
        &account.host,
        account.port,
        &secrets.username,
        &secrets.password,
    ) {
        Ok(session) => {
            info!(email = %account.email, "Login succeeded");
            return Ok(session);
        }
        Err(e) => e,
    };

    if account.host == "imap.gmail.com" && secrets.username.contains('@') {
        let local = secrets.username.split('@').next().unwrap();
        info!("Retrying Gmail login with local username: {}", local);
        match try_login(&account.host, account.port, local, &secrets.password) {
            Ok(session) => {
                info!(email = %account.email, "Login succeeded with local username");
                return Ok(session);
            }
            Err(e2) => {
                error!(
                    email = %account.email,
                    "Login failed: {:?} / retry: {:?}",
                    first_err,
                    e2
                );
                return Err(e2);
            }
        }
    }

    error!(email = %account.email, "Login failed: {:?}", first_err);
    Err(first_err)
}

/// Smoke-test an IMAP connection without persisting anything. Opens TLS,
/// logs in, immediately logs out. Returns Ok if the credentials work.
pub async fn test_connection(
    host: String,
    port: u16,
    username: String,
    password: String,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        let tls = TlsConnector::builder().build()?;
        let client = connect_with_timeout(&host, port, &tls)?;
        let mut session = client
            .login(&username, &password)
            .map_err(|(e, _)| anyhow::anyhow!("login failed: {:?}", e))?;
        let _ = session.logout();
        Ok::<(), anyhow::Error>(())
    })
    .await?
}

/// Sanitize a string so it can be used as a single filesystem path
/// component. IMAP-supplied mailbox names are otherwise joined directly
/// into the output path.
fn sanitize_path_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    match cleaned.as_str() {
        "" | "." | ".." => "_".to_string(),
        _ => cleaned,
    }
}
