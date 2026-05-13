//! Detect forwarded mail and pull out the original sender's address.
//!
//! Two flavors of "forwarded" exist:
//!
//! 1. **Permanent / server-side** — the user has a rule on (e.g.) `web.de`
//!    that auto-relays everything to gmail. The original sender's headers
//!    stay intact; the only fingerprint is the *envelope* (Return-Path,
//!    Received chain). The standard signal is SRS rewriting: the bounce
//!    address gets rewritten to `<SRS0=...@srs.web.de>` so SPF survives
//!    the forwarding hop. We key on that.
//!
//! 2. **Inline / manual** — someone hit "Forward" in their client. The
//!    wrapping `From:` is the forwarder; the original sender sits inside
//!    the body behind a sentinel ("---------- Forwarded message ----------",
//!    "Begin forwarded message:", various Outlook/Yahoo/web.de variants).
//!
//! For (1) the `From:` header IS the original sender — we populate
//! `original_sender_addr`/`_domain` directly from it. For (2) we extract a
//! domain from the body and leave the address column null.

use super::parser::ParsedMail;
use std::sync::OnceLock;

#[derive(Debug, Clone, Default)]
pub struct ForwardingInfo {
    pub is_forwarded: bool,
    /// The address the mail was forwarded *from* — typically the user's
    /// other mailbox (e.g. `ambien@web.de`) that relayed to this account.
    pub forwarded_from: Option<String>,
    pub forwarded_from_domain: Option<String>,
    /// The *original* sender's domain (e.g. `amazon.de`).
    pub original_sender_domain: Option<String>,
    /// The *original* sender's full address (e.g. `orders@amazon.de`).
    /// Only populated for permanent forwarding where the `From:` header is
    /// the original sender — inline forwards in the body don't expose it.
    pub original_sender_addr: Option<String>,
}

pub fn analyze(parsed: &ParsedMail) -> ForwardingInfo {
    let mut info = ForwardingInfo::default();

    // ── 1. Header signals ────────────────────────────────────────────────
    let mut srs_forwarder_domain: Option<String> = None;
    let mut received_forwarder_domain: Option<String> = None;

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
            "return-path" => {
                if let Some(domain) = srs_domain(value) {
                    info.is_forwarded = true;
                    srs_forwarder_domain = Some(domain);
                }
            }
            "received" => {
                if let Some(domain) = relay_domain(value) {
                    received_forwarder_domain.get_or_insert(domain);
                }
            }
            _ => {}
        }
    }

    // SRS Return-Path is the cleanest server-side-forwarding signal. If
    // we have it (or a known consumer relay in Received with another
    // corroborating address-domain match), treat as permanent forwarding.
    if info.forwarded_from.is_none() {
        let candidate_domain = srs_forwarder_domain
            .clone()
            .or_else(|| received_forwarder_domain.clone());
        if let Some(domain) = candidate_domain {
            if let Some(addr) = find_address_with_domain(parsed, &domain) {
                info.is_forwarded = true;
                info.forwarded_from_domain = Some(domain);
                info.forwarded_from = Some(addr);
            } else if srs_forwarder_domain.is_some() {
                // SRS alone is conclusive even without a To: hit (the
                // forwarder address may be on the BCC envelope only).
                info.is_forwarded = true;
                info.forwarded_from_domain = Some(domain);
            }
        }
    }

    // For permanent forwarding the `From:` header *is* the original sender.
    if info.is_forwarded && info.original_sender_addr.is_none() {
        if let Some(from) = parsed.from.as_ref() {
            let from_addr = from.email.to_ascii_lowercase();
            let from_domain = domain_of(&from_addr);
            // Don't credit ourselves as the original sender if the
            // wrapping mail genuinely came from the forwarder mailbox
            // (inline-forward case — handled below by body sentinel).
            let same_as_forwarder = info
                .forwarded_from
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case(&from_addr))
                .unwrap_or(false);
            if !same_as_forwarder {
                info.original_sender_addr = Some(from_addr);
                info.original_sender_domain = from_domain;
            }
        }
    }

    // ── 2. Body sentinels (inline / manual forwarding) ──────────────────
    if let Some(body) = parsed.body_text.as_deref() {
        if SENTINEL_RE
            .get_or_init(build_sentinel_re)
            .iter()
            .any(|needle| body.contains(needle))
        {
            info.is_forwarded = true;
        }

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

    // Fall back: flagged forwarded but never resolved the forwarder at all.
    // (Don't run this if SRS already gave us a domain — From: is the
    // original sender there, not the forwarder.)
    if info.is_forwarded && info.forwarded_from.is_none() && info.forwarded_from_domain.is_none() {
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

/// Return-Path local/domain patterns that mean "this mail was rewritten
/// by SRS, ergo permanently forwarded". Returns the *real* forwarder
/// domain (stripping any `srs.` prefix).
///
/// Examples we want to match:
///   <SRS0=hash=seg=user=origin.com@srs.web.de>      → "web.de"
///   <SRS0=hash=seg=user=origin.com@web.de>          → "web.de"
///   <prvs=...@srs.fastmail.com>                     → "fastmail.com"
fn srs_domain(return_path: &str) -> Option<String> {
    let addr = pluck_email(return_path)?;
    let (local, domain) = addr.split_once('@')?;
    let domain = domain.trim_end_matches('>').to_ascii_lowercase();
    let local_l = local.to_ascii_lowercase();
    let local_is_srs = local_l.starts_with("srs0=")
        || local_l.starts_with("srs1=")
        || local_l.starts_with("prvs=");
    let domain_is_srs = domain.starts_with("srs.");
    if local_is_srs || domain_is_srs {
        let stripped = domain.strip_prefix("srs.").unwrap_or(&domain);
        // Reject obvious noise: must look like a real domain (>= 2 labels).
        if stripped.contains('.') && stripped.len() > 3 {
            return Some(stripped.to_string());
        }
    }
    None
}

/// Known consumer-mail relay hostnames that show up in `Received: from`
/// when those providers forward to a third party. Returns the high-level
/// forwarder domain (e.g. "web.de" for "mout.web.de").
fn relay_domain(received: &str) -> Option<String> {
    // We only consider the very first "from <host>" token of the Received
    // header (the next-hop server). Anything else is just routing noise.
    let lower = received.to_ascii_lowercase();
    let from_idx = lower.find(" from ").or_else(|| {
        if lower.starts_with("from ") {
            Some(0)
        } else {
            None
        }
    })?;
    let after_from = &lower[from_idx + " from ".len() - 1..];
    let host: String = after_from
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .collect();
    if host.is_empty() {
        return None;
    }
    const FORWARDER_RELAYS: &[(&str, &str)] = &[
        ("mout.web.de", "web.de"),
        ("mout.gmx.net", "gmx.net"),
        ("mout.gmx.com", "gmx.com"),
        ("mout.kundenserver.de", "1und1.de"),
        ("mout.perfora.net", "1und1.de"),
        // Subdomain matches handled by `ends_with`.
    ];
    for (pat, fwd) in FORWARDER_RELAYS {
        if &host == pat || host.ends_with(&format!(".{pat}")) {
            return Some((*fwd).to_string());
        }
    }
    None
}

fn find_address_with_domain(parsed: &ParsedMail, domain: &str) -> Option<String> {
    let domain_l = domain.to_ascii_lowercase();
    // Prefer To:/Cc: (parsed) first.
    for addr in parsed.to.iter().chain(parsed.cc.iter()) {
        if let Some(d) = domain_of(&addr.email) {
            if d == domain_l {
                return Some(addr.email.to_ascii_lowercase());
            }
        }
    }
    // Then scan Delivered-To headers (envelope, not in parsed.to).
    for (k, v) in &parsed.all_headers {
        if !k.eq_ignore_ascii_case("delivered-to") && !k.eq_ignore_ascii_case("x-delivered-to") {
            continue;
        }
        if let Some(addr) = pluck_email(v) {
            if let Some(d) = domain_of(&addr) {
                if d == domain_l {
                    return Some(addr.to_ascii_lowercase());
                }
            }
        }
    }
    None
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

    fn mk_with_to(headers: &[(&str, &str)], from: Option<&str>, to: &[&str]) -> ParsedMail {
        ParsedMail {
            from: from.map(|e| Address {
                name: None,
                email: e.to_string(),
            }),
            to: to
                .iter()
                .map(|e| Address {
                    name: None,
                    email: (*e).to_string(),
                })
                .collect(),
            body_text: None,
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
    fn detects_srs_return_path_web_de() {
        // Permanent forwarding from web.de -> gmail. Real-world sample:
        //   Return-Path: <SRS0=...@srs.web.de>
        //   To: ambien@web.de
        //   From: orders@amazon.de
        let p = mk_with_to(
            &[
                ("Return-Path", "<SRS0=1jcj=3N=amazonses.com=foo@srs.web.de>"),
                (
                    "Received",
                    "from mout.web.de (mout.web.de. [212.227.17.12])",
                ),
            ],
            Some("orders@amazon.de"),
            &["ambien@web.de"],
        );
        let info = analyze(&p);
        assert!(info.is_forwarded);
        assert_eq!(info.forwarded_from.as_deref(), Some("ambien@web.de"));
        assert_eq!(info.forwarded_from_domain.as_deref(), Some("web.de"));
        assert_eq!(
            info.original_sender_addr.as_deref(),
            Some("orders@amazon.de")
        );
        assert_eq!(info.original_sender_domain.as_deref(), Some("amazon.de"));
    }

    #[test]
    fn srs_without_to_match_still_flagged() {
        // SRS but no To: header containing the web.de address (envelope-only).
        let p = mk(
            &[("Return-Path", "<SRS0=xyz=abc=foo@srs.web.de>")],
            "",
            Some("orders@amazon.de"),
        );
        let info = analyze(&p);
        assert!(info.is_forwarded);
        assert_eq!(info.forwarded_from_domain.as_deref(), Some("web.de"));
        // No specific address could be resolved, but domain is set.
        assert_eq!(
            info.original_sender_addr.as_deref(),
            Some("orders@amazon.de")
        );
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

    #[test]
    fn plain_mail_not_flagged() {
        let p = mk_with_to(
            &[("Return-Path", "<noreply@example.com>")],
            Some("noreply@example.com"),
            &["someone@gmail.com"],
        );
        let info = analyze(&p);
        assert!(!info.is_forwarded);
        assert!(info.original_sender_addr.is_none());
    }
}
