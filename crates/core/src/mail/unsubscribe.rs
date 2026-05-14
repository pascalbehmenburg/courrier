//! Parse RFC 2369 `List-Unsubscribe` + RFC 8058 `List-Unsubscribe-Post`.
//!
//! `List-Unsubscribe` is a comma-separated list of angle-bracketed URIs:
//!   List-Unsubscribe: <mailto:u@list.com?subject=unsub>, <https://x.com/u/abc>
//!
//! `List-Unsubscribe-Post: List-Unsubscribe=One-Click` (RFC 8058) means
//! the HTTPS URL is safe to POST to without further confirmation.

use super::parser::ParsedMail;

#[derive(Debug, Clone, Default)]
pub struct UnsubscribeInfo {
    /// HTTPS URL safe for a one-click POST (RFC 8058). Implies the mail
    /// also advertised `List-Unsubscribe-Post: List-Unsubscribe=One-Click`.
    pub one_click_url: Option<String>,
    /// mailto: target for manual / mailto-based unsubscribe.
    pub mailto: Option<String>,
    /// HTTPS URL without one-click — user has to visit the page manually.
    pub web_url: Option<String>,
}

impl UnsubscribeInfo {
    pub fn is_empty(&self) -> bool {
        self.one_click_url.is_none() && self.mailto.is_none() && self.web_url.is_none()
    }
}

pub fn extract(parsed: &ParsedMail) -> UnsubscribeInfo {
    let mut info = UnsubscribeInfo::default();

    // Pull the raw header values.
    let mut list_unsub_value: Option<&str> = None;
    let mut one_click_post = false;
    for (k, v) in &parsed.all_headers {
        if k.eq_ignore_ascii_case("list-unsubscribe") {
            list_unsub_value = Some(v.as_str());
        } else if k.eq_ignore_ascii_case("list-unsubscribe-post")
            && v.to_ascii_lowercase()
                .contains("list-unsubscribe=one-click")
        {
            one_click_post = true;
        }
    }
    let Some(raw) = list_unsub_value else {
        return info;
    };

    let (mailtos, webs): (Vec<&str>, Vec<&str>) = parse_uris(raw)
        .into_iter()
        .partition(|u| u.starts_with("mailto:"));

    info.mailto = mailtos.into_iter().next().map(|s| s.to_string());
    if let Some(web) = webs.into_iter().next() {
        if one_click_post {
            info.one_click_url = Some(web.to_string());
        } else {
            info.web_url = Some(web.to_string());
        }
    }
    info
}

/// Return the URI tokens from a `List-Unsubscribe` header. Tolerates the
/// common malformed cases (missing brackets, weird whitespace) by treating
/// anything that *looks* like a URL as one.
fn parse_uris(value: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the next '<'.
        let Some(start) = bytes[i..].iter().position(|b| *b == b'<') else {
            break;
        };
        let abs_start = i + start + 1;
        let Some(end_rel) = bytes[abs_start..].iter().position(|b| *b == b'>') else {
            break;
        };
        let abs_end = abs_start + end_rel;
        let slice = value[abs_start..abs_end].trim();
        if !slice.is_empty() {
            out.push(slice);
        }
        i = abs_end + 1;
    }
    // Fallback: if no angle brackets at all, accept comma-separated bare URIs.
    if out.is_empty() {
        for token in value.split(',') {
            let t = token.trim();
            if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("mailto:") {
                out.push(t);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(headers: &[(&str, &str)]) -> ParsedMail {
        ParsedMail {
            all_headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn extracts_one_click_https() {
        let p = mk(&[
            (
                "List-Unsubscribe",
                "<mailto:u-abc@list.com>, <https://list.com/unsub/abc>",
            ),
            ("List-Unsubscribe-Post", "List-Unsubscribe=One-Click"),
        ]);
        let u = extract(&p);
        assert_eq!(
            u.one_click_url.as_deref(),
            Some("https://list.com/unsub/abc")
        );
        assert_eq!(u.mailto.as_deref(), Some("mailto:u-abc@list.com"));
        assert!(u.web_url.is_none());
    }

    #[test]
    fn web_url_without_one_click_post() {
        let p = mk(&[(
            "List-Unsubscribe",
            "<https://list.com/unsub/abc>, <mailto:u@list.com>",
        )]);
        let u = extract(&p);
        assert!(u.one_click_url.is_none());
        assert_eq!(u.web_url.as_deref(), Some("https://list.com/unsub/abc"));
        assert_eq!(u.mailto.as_deref(), Some("mailto:u@list.com"));
    }

    #[test]
    fn mailto_only() {
        let p = mk(&[("List-Unsubscribe", "<mailto:u@list.com>")]);
        let u = extract(&p);
        assert!(u.one_click_url.is_none());
        assert!(u.web_url.is_none());
        assert_eq!(u.mailto.as_deref(), Some("mailto:u@list.com"));
    }

    #[test]
    fn none_when_header_absent() {
        let p = mk(&[("Subject", "hi")]);
        assert!(extract(&p).is_empty());
    }

    #[test]
    fn handles_no_brackets() {
        let p = mk(&[("List-Unsubscribe", "https://list.com/u, mailto:u@list.com")]);
        let u = extract(&p);
        assert_eq!(u.web_url.as_deref(), Some("https://list.com/u"));
        assert_eq!(u.mailto.as_deref(), Some("mailto:u@list.com"));
    }
}
