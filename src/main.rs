
use anyhow::Result;
extern crate imap;
extern crate native_tls;
use std::fs;
use std::path::PathBuf;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct AccountConfig {
    email: String,
    username: String,
    password: String,
    server: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
struct Account {
    email: String,
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    accounts: Vec<Account>,
}

fn default_port() -> u16 {
    993
}

#[derive(Debug, Deserialize)]
struct Config {
    servers: Vec<ServerConfig>,
}

fn connect_and_login(config: &AccountConfig) -> Result<imap::Session<native_tls::TlsStream<std::net::TcpStream>>> {
    let tls = native_tls::TlsConnector::builder().build()?;
    println!("Connecting to {}:{}", config.server, config.port);
    
    // Try login with configured username first
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
                println!("First attempt failed, reconnecting and trying with local username: {}", username_local);
                
                // Reconnect for retry (client was consumed by login attempt)
                let retry_client = imap::connect((config.server.as_str(), config.port), config.server.as_str(), &tls)?;
                
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

fn fetch_message_body(
    session: &mut imap::Session<native_tls::TlsStream<std::net::TcpStream>>,
    uid: u32,
    use_uid_fetch: bool,
) -> Result<Vec<u8>> {
    // Try BODY.PEEK[] first (most reliable, doesn't mark as seen)
    let body = if use_uid_fetch {
        match session.uid_fetch(uid.to_string(), "BODY.PEEK[]") {
            Ok(msgs) => {
                if let Some(msg) = msgs.iter().next() {
                    msg.body().map(|b| Vec::from(b))
                } else {
                    None
                }
            }
            Err(_) => None  // Will try RFC822 as fallback
        }
    } else {
        match session.fetch(uid.to_string(), "BODY.PEEK[]") {
            Ok(msgs) => {
                if let Some(msg) = msgs.iter().next() {
                    msg.body().map(|b| Vec::from(b))
                } else {
                    None
                }
            }
            Err(_) => None  // Will try RFC822 as fallback
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
                    Err(anyhow::anyhow!("Failed to fetch message body for UID {}: BODY.PEEK[] and RFC822 both returned no body", uid))
                }
            } else {
                Err(anyhow::anyhow!("Failed to fetch message body for UID {}: BODY.PEEK[] and RFC822 both returned no messages", uid))
            }
        }
        Err(e) => {
            Err(anyhow::anyhow!("Failed to fetch message body for UID {}: BODY.PEEK[] and RFC822 both failed. Last error: {:?}", uid, e))
        }
    }
}

fn fetch_all_messages_from_mailbox(
    config: &AccountConfig,
    mailbox_name: &str,
    output_dir: &PathBuf,
) -> Result<usize> {
    let mut session = connect_and_login(config)?;
    
    // List available mailboxes
    println!("Listing mailboxes...");
    let _mailboxes = session.list(Some(""), Some("*"))?;
    
    // Select/examine the mailbox
    println!("Selecting mailbox: {}...", mailbox_name);
    let mailbox = match session.select(mailbox_name) {
        Ok(m) => m,
        Err(_) => {
            println!("Select failed, trying EXAMINE...");
            session.examine(mailbox_name)?
        }
    };
    
    println!("✓ Selected {} ({} messages)", mailbox_name, mailbox.exists);
    
    // Get all UIDs
    let uids = session.uid_search("ALL")?;
    println!("Found {} messages to fetch", uids.len());
    
    if uids.is_empty() {
        println!("No messages in mailbox");
        return Ok(0);
    }
    
    // Create output directory for this account/mailbox
    let account_dir = output_dir.join(&config.email.replace("@", "_"));
    let mailbox_dir = account_dir.join(mailbox_name);
    fs::create_dir_all(&mailbox_dir)?;
    println!("Saving messages to: {}", mailbox_dir.display());
    
    let mut saved_count = 0;
    let mut failed_count = 0;
    
    // Fetch each message
    for (idx, uid) in uids.iter().enumerate() {
        print!("\rFetching message {}/{} (UID: {})...", idx + 1, uids.len(), uid);
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        
        match fetch_message_body(&mut session, *uid, true) {
            Ok(body) => {
                // Save as .eml file
                let filename = format!("{}.eml", uid);
                let filepath = mailbox_dir.join(&filename);
                
                match fs::write(&filepath, &body) {
                    Ok(_) => {
                        saved_count += 1;
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
    
    // Logout (ignore errors)
    let _ = session.logout();
    
    Ok(saved_count)
}

fn load_config_from_file(config_path: &PathBuf) -> Result<Vec<AccountConfig>> {
    let config_content = fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&config_content)?;
    
    let mut accounts = Vec::new();
    
    for server in config.servers {
        for account in server.accounts {
            accounts.push(AccountConfig {
                email: account.email,
                username: account.username,
                password: account.password,
                server: server.host.clone(),
                port: server.port,
            });
        }
    }
    
    if accounts.is_empty() {
        Err(anyhow::anyhow!("No accounts found in config file"))
    } else {
        Ok(accounts)
    }
}

fn main() -> Result<()> {
    // Load config from config.toml file
    let config_path = PathBuf::from("config.toml");
    
    if !config_path.exists() {
        return Err(anyhow::anyhow!(
            "Config file not found: {}\n\
            Please create a config.toml file with the following format:\n\
            \n\
            [[servers]]\n\
            host = \"imap.mail.me.com\"\n\
            port = 993\n\
            accounts = [\n\
              {{ email = \"your-email@example.com\", username = \"your-username\", password = \"your-password\" }},\n\
              {{ email = \"another-email@example.com\", username = \"another-username\", password = \"another-password\" }}\n\
            ]\n\
            \n\
            [[servers]]\n\
            host = \"imap.gmail.com\"\n\
            port = 993\n\
            accounts = [\n\
              {{ email = \"gmail-account@gmail.com\", username = \"gmail-username\", password = \"gmail-password\" }}\n\
            ]\n\
            \n\
            See config.toml.example for a complete example.",
            config_path.display()
        ));
    }
    
    let accounts = load_config_from_file(&config_path)?;
    println!("Loaded {} account(s) from {}", accounts.len(), config_path.display());
    
    // Create output directory
    let output_dir = PathBuf::from("emails");
    fs::create_dir_all(&output_dir)?;
    println!("Output directory: {}", output_dir.display());
    
    let mailboxes_to_fetch = vec!["INBOX", "Junk"];  // You can add more mailboxes here
    
    let mut total_saved = 0;
    
    // Process each account
    for account in &accounts {
        println!("\n{}", "=".repeat(80));
        println!("Processing account: {}", account.email);
        println!("{}", "=".repeat(80));
        
        for mailbox in &mailboxes_to_fetch {
            println!("\n--- Fetching from mailbox: {} ---", mailbox);
            
            match fetch_all_messages_from_mailbox(account, mailbox, &output_dir) {
                Ok(count) => {
                    println!("✓ Successfully saved {} messages from {}/{}", count, account.email, mailbox);
                    total_saved += count;
                }
                Err(e) => {
                    eprintln!("✗ Failed to fetch from {}/{}: {:?}", account.email, mailbox, e);
                }
            }
        }
    }
    
    println!("\n{}", "=".repeat(80));
    println!("✓ Done! Total messages saved: {}", total_saved);
    println!("Messages saved to: {}", output_dir.display());
    println!("{}", "=".repeat(80));
    
    Ok(())
}