//! Detect forwarded mail and pull out the original sender's address.
//!
//! A "forwarded" mail can take many shapes:
//!   - Resent-* headers (RFC 5322 — the cleanest signal)
//!   - X-Forwarded-* / X-Original-* headers
//!   - Body sentinels inserted by clients ("---------- Forwarded message ----------",
//!     "Begin forwarded message:", various Outlook / Yahoo / web.de variants)
//!   - "via" addressing (some servers rewrite From: as "real-sender via list")
//!
//! This is heuristics, not magic — we strive to be conservative (avoid
//! false positives) and to surface the *innermost* original sender we can
//! find in the body.

use super::parser::ParsedMail;
use std::sync::OnceLock;

#[derive(Debug, Clone, Default)]
pub struct ForwardingInfo {
    pub is_forwarded: bool,
    /// The address the mail was forwarded *from* — typically the sender of
    /// the wrapping forward (i.e. the user's other account that auto-forwarded).
    pub forwarded_from: Option<String>,
    pub forwarded_from_domain: Option<String>,
    /// The *original* sender we can extract from the inner forwarded body
    /// (e.g. amazon.de when ambien@web.de forwarded the Amazon email).
    pub original_sender_domain: Option<String>,
}

pub fn analyze(parsed: &ParsedMail) -> ForwardingInfo {
    let mut info = ForwardingInfo::default();

    // Header signals.
    for (key, value) in &parsed.all_headers {
        let k = key.to_ascii_lowercase();
        match k.as_str() {
            "resent-from" | "x-original-from" | "x-original-sender" | "x-forwarded-for" => {
                info.is_forwarded = true;
                if info.forwarded_from.is_none() {
                    if let Some(addr) = pluck_email(value) {
                        info.forwarded_from_domain = domain_of(&addr);
                        info.forwarded_from = Some(addr);
                    }
                }
            }
            "x-forwarded-to" | "delivered-to" | "x-delivered-to" => {
                // Distinct from From:; treat as a forwarding hint only if From
                // and Delivered-To don't match the same address.
                if let (Some(from), Some(delivered)) = (
                    parsed.from.as_ref().map(|a| a.email.to_ascii_lowercase()),
                    pluck_email(value).map(|s| s.to_ascii_lowercase()),
                ) {
                    if !from.is_empty() && !delivered.contains(&from) {
                        // Not by itself enough to flag; needs corroboration.
                    }
                }
            }
            _ => {}
        }
    }

    // Body sentinels.
    if let Some(body) = parsed.body_text.as_deref() {
        if SENTINEL_RE
            .get_or_init(build_sentinel_re)
            .iter()
            .any(|needle| body.contains(needle))
        {
            info.is_forwarded = true;
        }

        // Try to find an inner "From: foo@bar" that follows a forwarding
        // sentinel. We look at the slice following the sentinel and pull the
        // first address from a line starting with "From:" or "Von:".
        if info.original_sender_domain.is_none() {
            if let Some(addr) = inner_from_address(body) {
                info.original_sender_domain = domain_of(&addr);
                if info.forwarded_from.is_none() {
                    info.forwarded_from = parsed.from.as_ref().map(|a| a.email.clone());
                    info.forwarded_from_domain = info.forwarded_from.as_deref().and_then(domain_of);
                }
            }
        }
    }

    // If we marked forwarded but never resolved a "forwarded_from", fall
    // back to the wrapping From: so dashboards have something to group by.
    if info.is_forwarded && info.forwarded_from.is_none() {
        if let Some(from) = parsed.from.as_ref() {
            info.forwarded_from = Some(from.email.clone());
            info.forwarded_from_domain = domain_of(&from.email);
        }
    }

    info
}

static SENTINEL_RE: OnceLock<Vec<&'static str>> = OnceLock::new();

fn build_sentinel_re() -> Vec<&'static str> {
    vec![
        "---------- Forwarded message ----------",
        "---------- Forwarded message ---------",
        "Begin forwarded message:",
        "-------- Original Message --------",
        "-------- Forwarded Message --------",
        "-----Original Message-----",
        "Von: ",                     // German Outlook
        "Weitergeleitete Nachricht", // German Thunderbird
    ]
}

fn pluck_email(value: &str) -> Option<String> {
    // Look for "<x@y>" first, then bare "x@y".
    if let (Some(lt), Some(gt)) = (value.find('<'), value.find('>')) {
        if gt > lt + 1 {
            let candidate = &value[lt + 1..gt];
            if candidate.contains('@') {
                return Some(candidate.trim().to_string());
            }
        }
    }
    for token in value.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let token = token.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '@' && c != '.' && c != '-' && c != '_' && c != '+'
        });
        if token.contains('@') && token.len() > 3 {
            return Some(token.to_string());
        }
    }
    None
}

fn domain_of(addr: &str) -> Option<String> {
    addr.split_once('@')
        .map(|(_, domain)| domain.trim_end_matches('>').to_ascii_lowercase())
}

fn inner_from_address(body: &str) -> Option<String> {
    for line in body.lines().take(200) {
        let trimmed = line.trim_start_matches(['>', ' ', '\t']);
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("from:") || lower.starts_with("von:") {
            if let Some(addr) = pluck_email(trimmed) {
                return Some(addr);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::parser::Address;

    fn mk(headers: &[(&str, &str)], body: &str, from: Option<&str>) -> ParsedMail {
        ParsedMail {
            from: from.map(|e| Address {
                name: None,
                email: e.to_string(),
            }),
            body_text: Some(body.to_string()),
            all_headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn detects_resent_from() {
        let p = mk(
            &[("Resent-From", "amazon@amazon.de")],
            "",
            Some("ambien@web.de"),
        );
        let info = analyze(&p);
        assert!(info.is_forwarded);
        assert_eq!(info.forwarded_from.as_deref(), Some("amazon@amazon.de"));
    }

    #[test]
    fn detects_body_sentinel_with_inner_from() {
        let body = "\
Hi, see below.

---------- Forwarded message ----------
From: orders@amazon.de
To: ambien@web.de
Subject: Your order

Thanks for your order
";
        let p = mk(&[], body, Some("ambien@web.de"));
        let info = analyze(&p);
        assert!(info.is_forwarded);
        assert_eq!(info.original_sender_domain.as_deref(), Some("amazon.de"));
        assert_eq!(info.forwarded_from.as_deref(), Some("ambien@web.de"));
        assert_eq!(info.forwarded_from_domain.as_deref(), Some("web.de"));
    }

    #[test]
    fn quoted_inner_from_is_handled() {
        let body = "\
> Begin forwarded message:
>
> From: \"Amazon\" <orders@amazon.de>
> To: ambien@web.de
";
        let p = mk(&[], body, Some("ambien@web.de"));
        let info = analyze(&p);
        assert!(info.is_forwarded);
        assert_eq!(info.original_sender_domain.as_deref(), Some("amazon.de"));
    }
}
