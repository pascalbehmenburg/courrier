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

pub fn load_config_from_file(config_path: &PathBuf) -> Result<Vec<AccountConfig>> {
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

pub fn load_config() -> Result<Vec<AccountConfig>> {
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

