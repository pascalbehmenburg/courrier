use crate::config::AccountConfig;
use crate::database::Database;
use anyhow::Result;
use imap::{Client, Session};
use native_tls::{TlsConnector, TlsStream};
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, error, info, warn};

const IMAP_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const IMAP_IO_TIMEOUT: Duration = Duration::from_secs(120);

/// Open an IMAP-over-TLS connection with a bounded TCP-connect timeout and
/// per-syscall read/write timeouts. The bare `imap::connect` helper uses
/// `TcpStream::connect`, which can hang indefinitely against a slow or
/// silently-dropped server and would block the global fetch slot forever.
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

/// Sanitize a string so it can be used as a single filesystem path component.
/// Replaces path separators, control chars, and null bytes; collapses ".",
/// "..", and empty strings to "_". Necessary because mailbox names come from
/// the IMAP server and are otherwise joined directly into output paths.
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

fn fetch_message_body(
    session: &mut Session<TlsStream<TcpStream>>,
    uid: u32,
) -> Result<Vec<u8>> {
    // BODY.PEEK[] is preferred — it doesn't mark the message as seen — but
    // some servers don't support it; fall back to RFC822 in that case.
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
    try_fetch(session, uid, "RFC822").ok_or_else(|| {
        anyhow::anyhow!(
            "Failed to fetch message body for UID {}: BODY.PEEK[] and RFC822 both returned no data",
            uid
        )
    })
}

pub async fn fetch_all_messages_from_mailbox(
    config: &AccountConfig,
    mailbox_name: &str,
    output_dir: &Path,
    db: &Database,
) -> Result<usize> {
    // Get already fetched UIDs from database first (before blocking task)
    let fetched_uids = db.get_fetched_uids(&config.email, mailbox_name).await?;
    let fetched_set: std::collections::HashSet<u32> = fetched_uids.into_iter().collect();

    // Prepare data for blocking task
    let config_clone = config.clone();
    let mailbox_name_str = mailbox_name.to_string();
    let output_dir_clone = output_dir.to_path_buf();
    let email_clone = config.email.clone();

    // Run all IMAP operations in a single blocking task
    let (saved_count, saved_uids) = tokio::task::spawn_blocking(move || {
        let mut session = connect_and_login_sync(&config_clone)?;

        // Select/examine the mailbox
        debug!("Selecting mailbox: {}", mailbox_name_str);
        let mailbox = match session.select(mailbox_name_str.as_str()) {
            Ok(m) => m,
            Err(_) => {
                debug!("Select failed, trying EXAMINE");
                session.examine(mailbox_name_str.as_str())?
            }
        };

        info!(
            mailbox = %mailbox_name_str,
            messages = mailbox.exists,
            "Selected mailbox"
        );

        // Get all UIDs that are NOT DELETED
        // Using "NOT DELETED" instead of "ALL" to ensure we get all messages
        // that are actually available (Gmail and other servers may filter "ALL")
        let uids = session.uid_search("NOT DELETED")?;
        let fetched_set_clone = fetched_set.clone();
        let uids_to_fetch: Vec<u32> = uids
            .iter()
            .filter(|uid| !fetched_set_clone.contains(uid))
            .copied()
            .collect();
        info!(
            mailbox = %mailbox_name_str,
            visible = uids.len(),
            already_fetched = fetched_set_clone.len(),
            new = uids_to_fetch.len(),
            "Mailbox UID survey"
        );

        // Fetch all messages in this blocking task
        let mut saved_count = 0;
        let mut failed_count = 0;
        let mut saved_uids: Vec<(u32, PathBuf, usize)> = Vec::new();

        if !uids_to_fetch.is_empty() {
            // Create output directory for this account/mailbox. Both segments
            // are sanitized: mailbox name is supplied by the IMAP server and
            // could contain path separators or "..".
            let account_dir = output_dir_clone.join(sanitize_path_component(&email_clone));
            let mailbox_dir = account_dir.join(sanitize_path_component(&mailbox_name_str));
            fs::create_dir_all(&mailbox_dir)?;
            debug!("Saving messages to: {}", mailbox_dir.display());

            for (idx, uid) in uids_to_fetch.iter().enumerate() {
                debug!(
                    "Fetching message {}/{} (UID: {})",
                    idx + 1,
                    uids_to_fetch.len(),
                    uid
                );

                match fetch_message_body(&mut session, *uid) {
                    Ok(body) => {
                        // Save as .eml file
                        let filename = format!("{}.eml", uid);
                        let filepath = mailbox_dir.join(&filename);
                        let size_bytes = body.len();

                        match fs::write(&filepath, &body) {
                            Ok(_) => {
                                saved_count += 1;
                                saved_uids.push((*uid, filepath, size_bytes));
                            }
                            Err(e) => {
                                error!("Failed to save {}: {:?}", filepath.display(), e);
                                failed_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to fetch UID {}: {:?}", uid, e);
                        failed_count += 1;
                    }
                }
            }

            info!(
                mailbox = %mailbox_name_str,
                saved = saved_count,
                failed = failed_count,
                "Mailbox fetch completed"
            );
        } else {
            debug!(mailbox = %mailbox_name_str, "No new messages to fetch");
        }

        // Logout (ignore errors)
        let _ = session.logout();

        Ok::<(usize, Vec<(u32, PathBuf, usize)>), anyhow::Error>((saved_count, saved_uids))
    })
    .await??;

    // Update database with fetched emails (do this after blocking task)
    for (uid, filepath, size_bytes) in saved_uids {
        if let Err(e) = db
            .mark_email_fetched(&config.email, mailbox_name, uid, &filepath, size_bytes)
            .await
        {
            error!("Failed to record UID {} in database: {:?}", uid, e);
        }
    }

    Ok(saved_count)
}

/// Open a fresh TLS connection and authenticate. Pulled out so the Gmail
/// fallback path doesn't have to duplicate the connect/login dance.
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

fn log_gmail_troubleshooting() {
    warn!(
        "Gmail troubleshooting:\n\
         1. Ensure IMAP is enabled in Gmail settings\n\
         2. Use an App-Specific Password (not your regular password). \
            Generate one at: https://myaccount.google.com/apppasswords\n\
         3. If 2FA is disabled, enable it first (required for app passwords)\n\
         4. App passwords are 16 characters (may include spaces)"
    );
}

// Synchronous version for use in blocking tasks
fn connect_and_login_sync(config: &AccountConfig) -> Result<Session<TlsStream<TcpStream>>> {
    info!(
        server = %config.server,
        port = config.port,
        email = %config.email,
        username = %config.username,
        "Connecting to IMAP server"
    );

    let first_err = match try_login(
        &config.server,
        config.port,
        &config.username,
        &config.password,
    ) {
        Ok(session) => {
            info!(email = %config.email, "Login succeeded");
            return Ok(session);
        }
        Err(e) => e,
    };

    // For Gmail, if the configured username contains @, retry with the local
    // part — some account types only accept that form.
    if config.server == "imap.gmail.com" && config.username.contains('@') {
        let username_local = config.username.split('@').next().unwrap();
        info!("Retrying Gmail login with local username: {}", username_local);
        match try_login(&config.server, config.port, username_local, &config.password) {
            Ok(session) => {
                info!(email = %config.email, "Login succeeded with local username");
                return Ok(session);
            }
            Err(e2) => {
                error!(
                    email = %config.email,
                    "Login failed with both '{}' and '{}'. Original: {:?}. Retry: {:?}",
                    config.username, username_local, first_err, e2,
                );
                log_gmail_troubleshooting();
                return Err(e2);
            }
        }
    }

    error!(email = %config.email, "Login failed: {:?}", first_err);
    if config.server == "imap.gmail.com" {
        log_gmail_troubleshooting();
    }
    Err(first_err)
}

pub async fn fetch_all_accounts(
    accounts: &[AccountConfig],
    output_dir: &Path,
    db: &Database,
) -> Result<usize> {
    let run_id = db.start_fetch_run().await?;
    let result = fetch_all_accounts_inner(accounts, output_dir, db, run_id).await;
    let final_status = if result.is_ok() { "completed" } else { "failed" };
    if let Err(e) = db.complete_fetch_run(run_id, final_status).await {
        error!("Failed to mark fetch run {} complete: {:?}", run_id, e);
    }
    result
}

async fn fetch_all_accounts_inner(
    accounts: &[AccountConfig],
    output_dir: &Path,
    db: &Database,
    run_id: i64,
) -> Result<usize> {
    let mut total_saved = 0;

    for account in accounts {
        info!(account = %account.email, "Processing account");

        // Get all mailboxes from LIST command
        let account_clone = account.clone();
        let mailboxes = tokio::task::spawn_blocking(move || {
            let mut session = connect_and_login_sync(&account_clone)?;
            debug!("Listing all mailboxes");
            let mailboxes = session.list(Some(""), Some("*"))?;
            let _ = session.logout();
            Ok::<Vec<String>, anyhow::Error>(
                mailboxes.iter().map(|n| n.name().to_string()).collect(),
            )
        })
        .await??;

        info!(account = %account.email, mailboxes = mailboxes.len(), "Discovered mailboxes");
        debug!(account = %account.email, mailboxes = ?mailboxes, "Mailbox names");

        // Fetch from all mailboxes
        for mailbox in &mailboxes {
            match fetch_all_messages_from_mailbox(account, mailbox, output_dir, db).await {
                Ok(count) => {
                    info!(
                        account = %account.email,
                        mailbox = %mailbox,
                        saved = count,
                        "Mailbox synced"
                    );
                    total_saved += count;
                    if let Err(e) = db.record_fetch_run_progress(run_id, count as i64).await {
                        error!("Failed to record fetch progress: {:?}", e);
                    }
                }
                Err(e) => {
                    error!(
                        account = %account.email,
                        mailbox = %mailbox,
                        "Failed to fetch from mailbox: {:?}",
                        e
                    );
                }
            }
        }
    }

    Ok(total_saved)
}
