//! Wrap mailparse into something that lines up with our DB columns.

use anyhow::Result;
use chrono::{DateTime, Utc};
use mailparse::{addrparse, MailHeaderMap, ParsedMail as MpParsed};

use crate::database::MessageRow;
use crate::mail::forwarding;

#[derive(Debug, Clone)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedMail {
    pub message_id: Option<String>,
    pub subject: Option<String>,
    pub from: Option<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub date_utc: Option<DateTime<Utc>>,
    pub body_text: Option<String>,
    pub all_headers: Vec<(String, String)>,
}

impl ParsedMail {
    pub fn from_bytes(raw: &[u8]) -> Result<Self> {
        let parsed = mailparse::parse_mail(raw)?;
        Ok(extract(&parsed))
    }

    pub fn into_row(self, fetched_email_id: i64, account_id: i64, mailbox: String) -> MessageRow {
        let from_addr = self.from.as_ref().map(|a| a.email.clone());
        let from_name = self.from.as_ref().and_then(|a| a.name.clone());
        let to_addrs = serialize_addresses(&self.to);
        let cc_addrs = serialize_addresses(&self.cc);
        let body_size = self.body_text.as_deref().map(|s| s.len()).unwrap_or(0) as i64;

        let analysis = forwarding::analyze(&self);

        MessageRow {
            fetched_email_id,
            account_id,
            mailbox,
            message_id: self.message_id,
            subject: self.subject,
            from_addr,
            from_name,
            to_addrs,
            cc_addrs,
            date_utc: self.date_utc,
            body_text: self.body_text,
            is_forwarded: analysis.is_forwarded,
            forwarded_from: analysis.forwarded_from,
            forwarded_from_domain: analysis.forwarded_from_domain,
            original_sender_domain: analysis.original_sender_domain,
            size_bytes: body_size,
        }
    }
}

fn serialize_addresses(addrs: &[Address]) -> Option<String> {
    if addrs.is_empty() {
        None
    } else {
        Some(
            addrs
                .iter()
                .map(|a| match &a.name {
                    Some(n) => format!("{} <{}>", n, a.email),
                    None => a.email.clone(),
                })
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

fn extract(parsed: &MpParsed<'_>) -> ParsedMail {
    let headers = &parsed.headers;
    let all_headers: Vec<(String, String)> = headers
        .iter()
        .map(|h| (h.get_key(), h.get_value()))
        .collect();

    let message_id = headers.get_first_value("Message-ID").map(strip_brackets);
    let subject = headers.get_first_value("Subject");
    let from = headers
        .get_first_value("From")
        .and_then(|s| first_address(&s));
    let to = headers
        .get_first_value("To")
        .map(|s| all_addresses(&s))
        .unwrap_or_default();
    let cc = headers
        .get_first_value("Cc")
        .map(|s| all_addresses(&s))
        .unwrap_or_default();
    let date_utc = headers
        .get_first_value("Date")
        .and_then(|s| mailparse::dateparse(&s).ok())
        .and_then(|epoch| DateTime::from_timestamp(epoch, 0));

    let body_text = extract_text_body(parsed);

    ParsedMail {
        message_id,
        subject,
        from,
        to,
        cc,
        date_utc,
        body_text,
        all_headers,
    }
}

fn strip_brackets(s: String) -> String {
    s.trim_matches(|c: char| c == '<' || c == '>' || c.is_whitespace())
        .to_string()
}

fn first_address(s: &str) -> Option<Address> {
    addrparse(s).ok().and_then(|list| {
        list.iter().find_map(|a| match a {
            mailparse::MailAddr::Single(info) => Some(Address {
                name: info.display_name.clone(),
                email: info.addr.clone(),
            }),
            mailparse::MailAddr::Group(g) => g.addrs.first().map(|info| Address {
                name: info.display_name.clone(),
                email: info.addr.clone(),
            }),
        })
    })
}

fn all_addresses(s: &str) -> Vec<Address> {
    let mut out = Vec::new();
    if let Ok(list) = addrparse(s) {
        for entry in list.iter() {
            match entry {
                mailparse::MailAddr::Single(info) => out.push(Address {
                    name: info.display_name.clone(),
                    email: info.addr.clone(),
                }),
                mailparse::MailAddr::Group(g) => {
                    for info in &g.addrs {
                        out.push(Address {
                            name: info.display_name.clone(),
                            email: info.addr.clone(),
                        });
                    }
                }
            }
        }
    }
    out
}

/// Pull the first text/plain body from a (possibly multipart) message.
/// Falls back to a stripped HTML representation if no plain part exists.
fn extract_text_body(mail: &MpParsed<'_>) -> Option<String> {
    if let Some(text) = walk_for_mime(mail, "text/plain") {
        return Some(text);
    }
    walk_for_mime(mail, "text/html").map(strip_html)
}

fn walk_for_mime(mail: &MpParsed<'_>, target: &str) -> Option<String> {
    let mime = mail.ctype.mimetype.to_lowercase();
    if mime == target {
        return mail.get_body().ok();
    }
    for sub in &mail.subparts {
        if let Some(body) = walk_for_mime(sub, target) {
            return Some(body);
        }
    }
    None
}

/// Extremely loose HTML→text pass: drop tags, decode a handful of common
/// entities. Good enough for search indexing; not a substitute for a real
/// HTML renderer.
fn strip_html(html: String) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}
