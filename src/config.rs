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
    let config_path = PathBuf::from("Config.toml");

    if !config_path.exists() {
        return Err(anyhow::anyhow!(
            "Config file not found: {}\n\
            Please create a Config.toml file with the following format:\n\
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
            See Config.toml.example for a complete example.",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_config(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_load_valid_config() {
        let config_content = r#"
email_storage_path = "test_emails"
fetch_on_startup = false
fetch_interval_seconds = 3600

[[servers]]
host = "imap.example.com"
port = 993
accounts = [
    { email = "user@example.com", username = "user", password = "pass123" }
]
"#;
        let file = create_temp_config(config_content);
        let config = load_config_from_file(&file.path().to_path_buf()).unwrap();

        assert_eq!(config.email_storage_path, "test_emails");
        assert!(!config.fetch_on_startup);
        assert_eq!(config.fetch_interval_seconds, Some(3600));
        assert_eq!(config.servers.len(), 1);
        assert_eq!(config.servers[0].host, "imap.example.com");
        assert_eq!(config.servers[0].port, 993);
    }

    #[test]
    fn test_load_config_with_defaults() {
        let config_content = r#"
[[servers]]
host = "imap.example.com"
accounts = [
    { email = "user@example.com", username = "user", password = "pass" }
]
"#;
        let file = create_temp_config(config_content);
        let config = load_config_from_file(&file.path().to_path_buf()).unwrap();

        assert_eq!(config.email_storage_path, "emails");
        assert!(config.fetch_on_startup);
        assert_eq!(config.fetch_interval_seconds, None);
        assert_eq!(config.servers[0].port, 993);
    }

    #[test]
    fn test_load_config_multiple_servers() {
        let config_content = r#"
[[servers]]
host = "imap.gmail.com"
port = 993
accounts = [
    { email = "user1@gmail.com", username = "user1", password = "pass1" }
]

[[servers]]
host = "imap.outlook.com"
port = 993
accounts = [
    { email = "user2@outlook.com", username = "user2", password = "pass2" }
]
"#;
        let file = create_temp_config(config_content);
        let config = load_config_from_file(&file.path().to_path_buf()).unwrap();

        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.servers[0].host, "imap.gmail.com");
        assert_eq!(config.servers[1].host, "imap.outlook.com");
    }

    #[test]
    fn test_load_config_multiple_accounts_per_server() {
        let config_content = r#"
[[servers]]
host = "imap.example.com"
accounts = [
    { email = "user1@example.com", username = "user1", password = "pass1" },
    { email = "user2@example.com", username = "user2", password = "pass2" }
]
"#;
        let file = create_temp_config(config_content);
        let config = load_config_from_file(&file.path().to_path_buf()).unwrap();

        assert_eq!(config.servers[0].accounts.len(), 2);
    }

    #[test]
    fn test_extract_accounts_single_server() {
        let config_content = r#"
[[servers]]
host = "imap.example.com"
port = 993
accounts = [
    { email = "user@example.com", username = "user", password = "pass" }
]
"#;
        let file = create_temp_config(config_content);
        let config = load_config_from_file(&file.path().to_path_buf()).unwrap();
        let accounts = extract_accounts(&config);

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].email, "user@example.com");
        assert_eq!(accounts[0].username, "user");
        assert_eq!(accounts[0].password, "pass");
        assert_eq!(accounts[0].server, "imap.example.com");
        assert_eq!(accounts[0].port, 993);
    }

    #[test]
    fn test_extract_accounts_multiple_servers_and_accounts() {
        let config_content = r#"
[[servers]]
host = "imap.gmail.com"
port = 993
accounts = [
    { email = "user1@gmail.com", username = "user1", password = "pass1" },
    { email = "user2@gmail.com", username = "user2", password = "pass2" }
]

[[servers]]
host = "imap.outlook.com"
port = 143
accounts = [
    { email = "user3@outlook.com", username = "user3", password = "pass3" }
]
"#;
        let file = create_temp_config(config_content);
        let config = load_config_from_file(&file.path().to_path_buf()).unwrap();
        let accounts = extract_accounts(&config);

        assert_eq!(accounts.len(), 3);

        assert_eq!(accounts[0].email, "user1@gmail.com");
        assert_eq!(accounts[0].server, "imap.gmail.com");
        assert_eq!(accounts[0].port, 993);

        assert_eq!(accounts[1].email, "user2@gmail.com");
        assert_eq!(accounts[1].server, "imap.gmail.com");
        assert_eq!(accounts[1].port, 993);

        assert_eq!(accounts[2].email, "user3@outlook.com");
        assert_eq!(accounts[2].server, "imap.outlook.com");
        assert_eq!(accounts[2].port, 143);
    }

    #[test]
    fn test_extract_accounts_empty() {
        let config_content = r#"
[[servers]]
host = "imap.example.com"
accounts = []
"#;
        let file = create_temp_config(config_content);
        let config = load_config_from_file(&file.path().to_path_buf()).unwrap();
        let accounts = extract_accounts(&config);

        assert!(accounts.is_empty());
    }

    #[test]
    fn test_load_invalid_toml() {
        let config_content = "this is not valid toml {{{";
        let file = create_temp_config(config_content);
        let result = load_config_from_file(&file.path().to_path_buf());

        assert!(result.is_err());
    }

    #[test]
    fn test_load_missing_required_fields() {
        let config_content = r#"
[[servers]]
port = 993
accounts = []
"#;
        let file = create_temp_config(config_content);
        let result = load_config_from_file(&file.path().to_path_buf());

        assert!(result.is_err());
    }
}
