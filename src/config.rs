use anyhow::Result;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct AccountConfig {
    pub email: String,
    pub username: String,
    pub password: String,
    pub server: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
struct Account {
    email: String,
    username: String,
    password: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerConfig {
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    accounts: Vec<Account>,
}

fn default_port() -> u16 {
    993
}

fn default_email_storage_path() -> String {
    "emails".to_string()
}

fn default_fetch_on_startup() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_email_storage_path")]
    pub email_storage_path: String,
    pub fetch_interval_seconds: Option<u64>,
    #[serde(default = "default_fetch_on_startup")]
    pub fetch_on_startup: bool,
    pub(self) servers: Vec<ServerConfig>,
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default = "default_email_storage_path")]
    email_storage_path: String,
    fetch_interval_seconds: Option<u64>,
    #[serde(default = "default_fetch_on_startup")]
    fetch_on_startup: bool,
    servers: Vec<ServerConfig>,
}

pub fn load_config_from_file(config_path: &PathBuf) -> Result<AppConfig> {
    let config_content = fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&config_content)?;

    Ok(AppConfig {
        email_storage_path: config.email_storage_path,
        fetch_interval_seconds: config.fetch_interval_seconds,
        fetch_on_startup: config.fetch_on_startup,
        servers: config.servers,
    })
}

pub fn load_config() -> Result<AppConfig> {
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

    load_config_from_file(&config_path)
}

pub fn extract_accounts(config: &AppConfig) -> Vec<AccountConfig> {
    let mut accounts = Vec::new();

    for server in &config.servers {
        for account in &server.accounts {
            accounts.push(AccountConfig {
                email: account.email.clone(),
                username: account.username.clone(),
                password: account.password.clone(),
                server: server.host.clone(),
                port: server.port,
            });
        }
    }

    accounts
}

