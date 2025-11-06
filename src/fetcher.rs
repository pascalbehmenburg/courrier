use anyhow::Result;
use crate::config::AccountConfig;
use crate::database::Database;
use imap::Session;
use native_tls::TlsStream;
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};

fn fetch_message_body(
    session: &mut Session<TlsStream<TcpStream>>,
    uid: u32,
    use_uid_fetch: bool,
) -> Result<Vec<u8>> {
    // Try BODY.PEEK[] first (most reliable, doesn't mark as seen)
    let body = if use_uid_fetch {
        match session.uid_fetch(uid.to_string(), "BODY.PEEK[]") {
            Ok(msgs) => {
                if let Some(msg) = msgs.iter().next() {
                    msg.body().map(Vec::from)
                } else {
                    None
                }
            }
            Err(_) => None, // Will try RFC822 as fallback
        }
    } else {
        match session.fetch(uid.to_string(), "BODY.PEEK[]") {
            Ok(msgs) => {
                if let Some(msg) = msgs.iter().next() {
                    msg.body().map(Vec::from)
                } else {
                    None
                }
            }
            Err(_) => None, // Will try RFC822 as fallback
        }
    };

    // If BODY.PEEK[] succeeded, return the body
    if let Some(body) = body {
        return Ok(body);
    }

    // BODY.PEEK[] didn't work (either failed or returned no body), try RFC822
    let rfc822_result = if use_uid_fetch {
        session.uid_fetch(uid.to_string(), "RFC822")
    } else {
        session.fetch(uid.to_string(), "RFC822")
    };

    match rfc822_result {
        Ok(msgs) => {
            if let Some(msg) = msgs.iter().next() {
                if let Some(body) = msg.body() {
                    Ok(Vec::from(body))
                } else {
                    Err(anyhow::anyhow!(
                        "Failed to fetch message body for UID {}: BODY.PEEK[] and RFC822 both returned no body",
                        uid
                    ))
                }
            } else {
                Err(anyhow::anyhow!(
                    "Failed to fetch message body for UID {}: BODY.PEEK[] and RFC822 both returned no messages",
                    uid
                ))
            }
        }
        Err(e) => Err(anyhow::anyhow!(
            "Failed to fetch message body for UID {}: BODY.PEEK[] and RFC822 both failed. Last error: {:?}",
            uid,
            e
        )),
    }
}

pub async fn fetch_all_messages_from_mailbox(
    config: &AccountConfig,
    mailbox_name: &str,
    output_dir: &Path,
    db: &Database,
) -> Result<usize> {
    // Get already fetched UIDs from database first (before blocking task)
    let fetched_uids = db.get_fetched_uids(&config.email, mailbox_name)?;
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
        println!("Selecting mailbox: {}...", mailbox_name_str);
        let mailbox = match session.select(mailbox_name_str.as_str()) {
            Ok(m) => m,
            Err(_) => {
                println!("Select failed, trying EXAMINE...");
                session.examine(mailbox_name_str.as_str())?
            }
        };

        println!("✓ Selected {} ({} messages)", mailbox_name_str, mailbox.exists);

        // Get all UIDs that are NOT DELETED
        // Using "NOT DELETED" instead of "ALL" to ensure we get all messages
        // that are actually available (Gmail and other servers may filter "ALL")
        let uids = session.uid_search("NOT DELETED")?;
        println!("Found {} messages to fetch (NOT DELETED)", uids.len());

        // Filter out already fetched UIDs
        let fetched_set_clone = fetched_set.clone();
        let uids_to_fetch: Vec<u32> = uids
            .iter()
            .filter(|uid| !fetched_set_clone.contains(uid))
            .copied()
            .collect();

        println!(
            "Already fetched: {}, New to fetch: {}",
            fetched_set_clone.len(),
            uids_to_fetch.len()
        );

        // Fetch all messages in this blocking task
        let mut saved_count = 0;
        let mut failed_count = 0;
        let mut saved_uids: Vec<(u32, PathBuf, usize)> = Vec::new();

        if !uids_to_fetch.is_empty() {
            // Create output directory for this account/mailbox
            let account_dir = output_dir_clone.join(email_clone.replace("@", "_"));
            let mailbox_dir = account_dir.join(mailbox_name_str.as_str());
            fs::create_dir_all(&mailbox_dir)?;
            println!("Saving messages to: {}", mailbox_dir.display());

            for (idx, uid) in uids_to_fetch.iter().enumerate() {
                print!(
                    "\rFetching message {}/{} (UID: {})...",
                    idx + 1,
                    uids_to_fetch.len(),
                    uid
                );
                std::io::stdout().flush().unwrap();

                match fetch_message_body(&mut session, *uid, true) {
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
                                eprintln!("\n✗ Failed to save {}: {:?}", filepath.display(), e);
                                failed_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("\n✗ Failed to fetch UID {}: {:?}", uid, e);
                        failed_count += 1;
                    }
                }
            }

            println!("\n✓ Completed: {} saved, {} failed", saved_count, failed_count);
        } else {
            println!("No new messages to fetch");
        }

        // Logout (ignore errors)
        let _ = session.logout();

        Ok::<(usize, Vec<(u32, PathBuf, usize)>), anyhow::Error>((saved_count, saved_uids))
    })
    .await??;

    // Update database with fetched emails (do this after blocking task)
    for (uid, filepath, size_bytes) in saved_uids {
        if let Err(e) = db.mark_email_fetched(
            &config.email,
            mailbox_name,
            uid,
            &filepath,
            size_bytes,
        ) {
            eprintln!("✗ Failed to record UID {} in database: {:?}", uid, e);
        }
    }

    Ok(saved_count)
}

// Synchronous version for use in blocking tasks
fn connect_and_login_sync(config: &AccountConfig) -> Result<Session<TlsStream<TcpStream>>> {
    let tls = native_tls::TlsConnector::builder().build()?;
    println!("Connecting to {}:{}", config.server, config.port);

    let client = imap::connect((config.server.as_str(), config.port), config.server.as_str(), &tls)?;
    println!("Connected to {}", config.server);
    println!("Logging in as {} (username: {})", config.email, config.username);

    match client.login(&config.username, &config.password) {
        Ok(session) => {
            println!("✓ Successfully logged in!");
            Ok(session)
        }
        Err(e) => {
            // For Gmail, if login fails and username contains @, try without the domain
            if config.server == "imap.gmail.com" && config.username.contains('@') {
                let username_local = config.username.split('@').next().unwrap();
                println!(
                    "First attempt failed, reconnecting and trying with local username: {}",
                    username_local
                );

                // Reconnect for retry
                let tls_retry = native_tls::TlsConnector::builder().build()?;
                let retry_client = imap::connect((config.server.as_str(), config.port), config.server.as_str(), &tls_retry)?;

                match retry_client.login(username_local, &config.password) {
                    Ok(session) => {
                        println!("✓ Successfully logged in with local username!");
                        Ok(session)
                    }
                    Err(e2) => {
                        eprintln!("❌ Login failed for {} with both username formats", config.email);
                        eprintln!("   Error with '{}': {:?}", config.username, e);
                        eprintln!("   Error with '{}': {:?}", username_local, e2);
                        eprintln!("\nGmail troubleshooting:");
                        eprintln!("1. Ensure IMAP is enabled in Gmail settings");
                        eprintln!("2. Use an App-Specific Password (not your regular password)");
                        eprintln!("   Generate one at: https://myaccount.google.com/apppasswords");
                        eprintln!("3. If 2FA is disabled, enable it first (required for app passwords)");
                        eprintln!("4. App passwords are 16 characters (may include spaces)");
                        Err(anyhow::anyhow!("Login failed: {:?}", e2.0))
                    }
                }
            } else {
                // For non-Gmail, just report the error
                eprintln!("❌ Login failed for {}: {:?}", config.email, e);
                if config.server == "imap.gmail.com" {
                    eprintln!("\nGmail troubleshooting:");
                    eprintln!("1. Ensure IMAP is enabled in Gmail settings");
                    eprintln!("2. Use an App-Specific Password (not your regular password)");
                    eprintln!("   Generate one at: https://myaccount.google.com/apppasswords");
                    eprintln!("3. If 2FA is disabled, enable it first (required for app passwords)");
                    eprintln!("4. App passwords are 16 characters (may include spaces)");
                }
                Err(anyhow::anyhow!("Login failed: {:?}", e.0))
            }
        }
    }
}

pub async fn fetch_all_accounts(
    accounts: &[AccountConfig],
    output_dir: &Path,
    db: &Database,
) -> Result<usize> {
    let mut total_saved = 0;

    for account in accounts {
        println!("\n{}", "=".repeat(80));
        println!("Processing account: {}", account.email);
        println!("{}", "=".repeat(80));

        // Get all mailboxes from LIST command
        let account_clone = account.clone();
        let mailboxes = tokio::task::spawn_blocking(move || {
            let mut session = connect_and_login_sync(&account_clone)?;
            println!("Listing all mailboxes...");
            let mailboxes = session.list(Some(""), Some("*"))?;
            let _ = session.logout();
            
            // Extract mailbox names from the LIST response
            let mailbox_names: Vec<String> = mailboxes
                .iter()
                .map(|name| name.name().to_string())
                .collect();
            
            Ok::<Vec<String>, anyhow::Error>(mailbox_names)
        })
        .await??;

        println!("Found {} mailbox(es):", mailboxes.len());
        for mailbox_name in &mailboxes {
            println!("  - {}", mailbox_name);
        }

        // Fetch from all mailboxes
        for mailbox in &mailboxes {
            println!("\n--- Fetching from mailbox: {} ---", mailbox);

            match fetch_all_messages_from_mailbox(account, mailbox, output_dir, db).await {
                Ok(count) => {
                    println!(
                        "✓ Successfully saved {} messages from {}/{}",
                        count, account.email, mailbox
                    );
                    total_saved += count;
                }
                Err(e) => {
                    eprintln!("✗ Failed to fetch from {}/{}: {:?}", account.email, mailbox, e);
                }
            }
        }
    }

    Ok(total_saved)
}

