//! IMAP provider presets.
//!
//! Most users never touch a config file again — they pick "iCloud", type
//! their email + app password, and we fill the rest. The list is small on
//! purpose: every entry has been verified to work with TLS on 993 and
//! standard LOGIN auth (no provider here requires OAuth for IMAP).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsernameStyle {
    /// Username is the full email address (Gmail, Outlook, Yahoo, Fastmail).
    FullEmail,
    /// Username is only the local-part of the email (iCloud, some legacy hosts).
    LocalPart,
    /// User must enter the username manually.
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub host: &'static str,
    pub port: u16,
    pub username_style: UsernameStyle,
    /// User-facing instructions, shown next to the password field.
    pub app_password_url: Option<&'static str>,
    pub notes: &'static str,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "icloud",
        label: "iCloud Mail",
        host: "imap.mail.me.com",
        port: 993,
        username_style: UsernameStyle::LocalPart,
        app_password_url: Some("https://account.apple.com/account/manage"),
        notes: "Generate an app-specific password from your Apple ID page. \
                Username is the part before @icloud.com / @me.com.",
    },
    Provider {
        id: "gmail",
        label: "Google (Gmail / Workspace)",
        host: "imap.gmail.com",
        port: 993,
        username_style: UsernameStyle::FullEmail,
        app_password_url: Some("https://myaccount.google.com/apppasswords"),
        notes: "Enable 2FA, then create a 16-character app password. \
                IMAP must be enabled in Gmail settings (Settings → Forwarding and POP/IMAP).",
    },
    Provider {
        id: "outlook",
        label: "Outlook / Microsoft 365",
        host: "outlook.office365.com",
        port: 993,
        username_style: UsernameStyle::FullEmail,
        app_password_url: Some("https://account.microsoft.com/security"),
        notes: "Personal accounts: enable 2FA and create an app password. \
                Tenant accounts may need IMAP enabled by an admin.",
    },
    Provider {
        id: "yahoo",
        label: "Yahoo Mail",
        host: "imap.mail.yahoo.com",
        port: 993,
        username_style: UsernameStyle::FullEmail,
        app_password_url: Some("https://login.yahoo.com/account/security"),
        notes: "Generate an app password from Account Security.",
    },
    Provider {
        id: "fastmail",
        label: "Fastmail",
        host: "imap.fastmail.com",
        port: 993,
        username_style: UsernameStyle::FullEmail,
        app_password_url: Some("https://www.fastmail.com/settings/security/tokens"),
        notes: "Create a dedicated app password with IMAP scope.",
    },
    Provider {
        id: "web_de",
        label: "Web.de",
        host: "imap.web.de",
        port: 993,
        username_style: UsernameStyle::FullEmail,
        app_password_url: None,
        notes: "Enable POP3/IMAP access in web.de mail settings.",
    },
    Provider {
        id: "gmx",
        label: "GMX",
        host: "imap.gmx.net",
        port: 993,
        username_style: UsernameStyle::FullEmail,
        app_password_url: None,
        notes: "Enable POP3/IMAP access in GMX mail settings.",
    },
    Provider {
        id: "custom",
        label: "Custom IMAP",
        host: "",
        port: 993,
        username_style: UsernameStyle::Manual,
        app_password_url: None,
        notes: "Enter host, port, and username manually.",
    },
];

pub fn find(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Apply provider defaults to derive a username from an email address.
pub fn derive_username(provider_id: &str, email: &str) -> Option<String> {
    let provider = find(provider_id)?;
    match provider.username_style {
        UsernameStyle::FullEmail => Some(email.to_string()),
        UsernameStyle::LocalPart => email.split('@').next().map(|s| s.to_string()),
        UsernameStyle::Manual => None,
    }
}
