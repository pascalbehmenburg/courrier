//! One-shot migrator from the pre-0.2 layout (Config.toml + emails/ tree)
//! to the new DB-managed schema.
//!
//! Reads the legacy Config.toml, inserts an `accounts` row per (server,
//! account) with the password AES-GCM-encrypted under
//! `COURRIER_ENCRYPTION_KEY`, then walks the on-disk
//! `<emails>/<sanitize(email)>/<sanitize(mailbox)>/<uid>.eml` tree and
//! inserts a `fetched_emails` row per file. The server's existing
//! `backfill_parser` will populate `messages` + FTS at the next boot.
//!
//! Usage:
//!   COURRIER_ENCRYPTION_KEY=$KEY \
//!     courrier-migrate --config Config.toml \
//!                      --emails /data/emails \
//!                      --db /data/courrier.db
//!
//! The --emails path must match what the server will see at runtime: the
//! file paths recorded in `fetched_emails` are stored as-is and read back
//! by the server at startup.

use anyhow::{Context, Result};
use base64::Engine;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use courrier_core::database::AccountInput;
use courrier_core::{Database, Encryptor};

#[derive(Debug, Deserialize)]
struct LegacyAccount {
    email: String,
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LegacyServer {
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    accounts: Vec<LegacyAccount>,
}

fn default_port() -> u16 {
    993
}

#[derive(Debug, Deserialize)]
struct LegacyConfig {
    #[serde(default)]
    servers: Vec<LegacyServer>,
}

struct Args {
    config: PathBuf,
    emails: PathBuf,
    db: PathBuf,
}

fn parse_args() -> Result<Args> {
    let mut config: Option<PathBuf> = None;
    let mut emails: Option<PathBuf> = None;
    let mut db: Option<PathBuf> = None;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => config = it.next().map(PathBuf::from),
            "--emails" => emails = it.next().map(PathBuf::from),
            "--db" => db = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unexpected argument: {other}"),
        }
    }
    Ok(Args {
        config: config.unwrap_or_else(|| PathBuf::from("Config.toml")),
        emails: emails.unwrap_or_else(|| PathBuf::from("emails")),
        db: db.unwrap_or_else(|| PathBuf::from("courrier.db")),
    })
}

fn print_help() {
    eprintln!("courrier-migrate — import legacy Config.toml + emails/ into the new DB");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  courrier-migrate [--config Config.toml] [--emails emails] [--db courrier.db]");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  COURRIER_ENCRYPTION_KEY    base64(32 random bytes) — must match the");
    eprintln!("                             value the server will run with");
}

/// Mirrors `fetcher::sanitize_path_component` so we look in the same
/// directories the legacy fetcher wrote into.
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

fn provider_for_host(host: &str) -> &'static str {
    match host {
        "imap.mail.me.com" => "icloud",
        "imap.gmail.com" => "gmail",
        "outlook.office365.com" => "outlook",
        "imap.mail.yahoo.com" => "yahoo",
        "imap.fastmail.com" => "fastmail",
        "imap.web.de" => "web_de",
        "imap.gmx.net" => "gmx",
        _ => "custom",
    }
}

fn load_key() -> Result<[u8; 32]> {
    let raw = std::env::var("COURRIER_ENCRYPTION_KEY")
        .context("COURRIER_ENCRYPTION_KEY is required (use the same key the server runs with)")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .context("COURRIER_ENCRYPTION_KEY must be valid base64")?;
    anyhow::ensure!(
        decoded.len() == 32,
        "COURRIER_ENCRYPTION_KEY must decode to exactly 32 bytes (got {})",
        decoded.len()
    );
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    Ok(key)
}

/// One .eml on disk, with the mailbox name reconstructed from its
/// directory path relative to the account root.
struct EmlEntry {
    mailbox: String,
    uid: u32,
    path: PathBuf,
    size: usize,
}

/// Walk every subdirectory under `account_dir`, collecting one entry per
/// `<uid>.eml` file. Mailbox name = relative path from `account_dir` to the
/// containing directory, joined by `/`. This handles both the flat layout
/// the post-split fetcher writes (`<account>/<sanitize(mailbox)>/<uid>.eml`)
/// and the older nested layout where the IMAP hierarchy was mirrored on
/// disk verbatim (`<account>/[Gmail]/All Mail/<uid>.eml`). For the nested
/// case the resulting mailbox name matches the IMAP-LIST name exactly, so
/// the new fetcher's UID-skip lookup hits on first sync.
fn collect_emls(account_dir: &Path) -> Result<Vec<EmlEntry>> {
    let mut out = Vec::new();
    let mut stack = vec![account_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?
        {
            let entry = entry?;
            let ft = entry.file_type()?;
            let path = entry.path();
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            if !ft.is_file() || path.extension().and_then(|s| s.to_str()) != Some("eml") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(uid) = stem.parse::<u32>() else {
                continue;
            };
            let rel = dir.strip_prefix(account_dir).unwrap_or(&dir);
            let mailbox = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            if mailbox.is_empty() {
                // .eml dumped directly under the account root has no mailbox.
                continue;
            }
            let size = std::fs::metadata(&path)?.len() as usize;
            out.push(EmlEntry {
                mailbox,
                uid,
                path,
                size,
            });
        }
    }
    Ok(out)
}

async fn migrate_account_files(
    db: &Database,
    account_id: i64,
    email: &str,
    emails_root: &Path,
) -> Result<usize> {
    let account_dir = emails_root.join(sanitize_path_component(email));
    if !account_dir.exists() {
        eprintln!(
            "  no directory at {} — skipping file import",
            account_dir.display()
        );
        return Ok(0);
    }

    let entries = collect_emls(&account_dir)?;
    let mut per_mailbox: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0usize;
    for e in entries {
        *per_mailbox.entry(e.mailbox.clone()).or_default() += 1;
        db.mark_email_fetched(account_id, &e.mailbox, e.uid, &e.path, e.size)
            .await?;
        total += 1;
    }
    for (mailbox, count) in per_mailbox {
        println!("  {email}/{mailbox}: {count} files");
    }
    Ok(total)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let key = load_key()?;
    let encryptor = Encryptor::new(&key);

    let cfg_text = std::fs::read_to_string(&args.config)
        .with_context(|| format!("reading {}", args.config.display()))?;
    let cfg: LegacyConfig =
        toml::from_str(&cfg_text).with_context(|| format!("parsing {}", args.config.display()))?;

    let db = Database::new(&args.db).with_context(|| format!("opening {}", args.db.display()))?;

    let existing: HashMap<String, i64> = db
        .list_accounts()
        .await?
        .into_iter()
        .map(|a| (a.email.clone(), a.id))
        .collect();

    let mut accounts_inserted = 0usize;
    let mut accounts_reused = 0usize;
    let mut fetches_inserted = 0usize;

    for server in cfg.servers {
        let provider_id = provider_for_host(&server.host).to_string();
        for acct in server.accounts {
            let account_id = if let Some(id) = existing.get(&acct.email) {
                println!("Account {} already in DB (id={id}), reusing", acct.email);
                accounts_reused += 1;
                *id
            } else {
                let ciphertext = encryptor.encrypt(&acct.password)?;
                let input = AccountInput {
                    label: acct.email.clone(),
                    email: acct.email.clone(),
                    username: acct.username.clone(),
                    host: server.host.clone(),
                    port: server.port,
                    provider_id: provider_id.clone(),
                    sync_interval_seconds: None,
                    enabled: true,
                    password_ciphertext: ciphertext,
                };
                let row = db.insert_account(input).await?;
                println!(
                    "Inserted account {} (id={}, host={}, provider={})",
                    row.email, row.id, row.host, row.provider_id
                );
                accounts_inserted += 1;
                row.id
            };

            fetches_inserted +=
                migrate_account_files(&db, account_id, &acct.email, &args.emails).await?;
        }
    }

    println!();
    println!("Done.");
    println!("  accounts inserted:        {accounts_inserted}");
    println!("  accounts already present: {accounts_reused}");
    println!("  fetched_emails rows:      {fetches_inserted}");
    println!();
    println!("Next: start the server with the same COURRIER_ENCRYPTION_KEY.");
    println!("Its backfill_parser will read each .eml and populate `messages`");
    println!("and the FTS index at boot. No IMAP traffic required.");

    Ok(())
}
