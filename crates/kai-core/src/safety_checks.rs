//! Kai's own safety checks, in the shape OpenAI's API sends
//! (`pending_safety_checks`), so open models and GPT share one approval
//! flow (`docs/RESEARCH_2026_10_OPENAI_PARITY.md` §5f). Off by default
//! (`--local-safety-checks`); they only ever add stops.
//!
//! - `irrelevant_domain`: the task names sites, and this page is on none of
//!   them, nor on one the user already allowed this run.
//! - `sensitive_domain`: webmail, payments, password managers, cloud
//!   consoles, government ID, health: a shipped list the user can override.
//!
//! `malicious_instructions` has no local rule here on purpose: Kai already
//! pauses on injection and phishing text before the model is asked
//! (`untrusted::suspicious`), and asking twice would only train people to
//! click through.
use crate::cua_protocol::SafetyCheck;
use std::sync::OnceLock;

const SENSITIVE_DEFAULT: &str = include_str!("../data/sensitive_domains.txt");

/// Second-level suffixes under which the registrable domain has three labels
/// (`bbc.co.uk`). A short list, not the Public Suffix List: an unknown suffix
/// errs toward a larger domain (`foo.co.zz` is treated as `co.zz`), which
/// only adds a check.
const MULTI: &[&str] = &[
    "co.uk", "org.uk", "ac.uk", "gov.uk", "co.in", "ac.in", "gov.in", "org.in", "net.in", "co.jp",
    "ne.jp", "or.jp", "com.au", "net.au", "org.au", "com.br", "com.cn", "com.mx", "co.nz", "co.za",
    "com.sg", "com.tr",
];

/// The host of a web address, lowercase, without `www.` or a port.
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    let host = host
        .split(':')
        .next()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    (host.contains('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'))
    .then_some(host)
}

/// The site a host belongs to: `docs.github.com` → `github.com`.
pub fn registrable(host: &str) -> String {
    let labels: Vec<&str> = host.split('.').collect();
    let keep = match labels.len() {
        n if n >= 3 && MULTI.contains(&labels[n - 2..].join(".").as_str()) => 3,
        _ => 2,
    };
    labels[labels.len().saturating_sub(keep)..].join(".")
}

/// Sites the task names, as `example.com` or a full address.
pub fn named_in(task: &str) -> Vec<String> {
    let mut sites: Vec<String> = task
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '"' | '\''))
        .map(|w| w.trim_end_matches(['.', '!', '?', ':']))
        .filter(|w| w.contains('.') && w.chars().any(|c| c.is_ascii_alphabetic()))
        .filter_map(host_of)
        // "e.g" or "v1.2": a site has a letter-only top-level label.
        .filter(|h| {
            h.rsplit('.')
                .next()
                .is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
        })
        .map(|h| registrable(&h))
        .collect();
    sites.dedup();
    sites
}

/// What this run may visit without a check.
#[derive(Debug, Default, Clone)]
pub struct RunDomains {
    named: Vec<String>,
    allowed: Vec<String>,
    acknowledged: Vec<String>,
}

impl RunDomains {
    pub fn new(task: &str) -> Self {
        Self {
            named: named_in(task),
            ..Default::default()
        }
    }

    /// The user accepted these checks: never ask about them again this run.
    pub fn accept(&mut self, checks: &[SafetyCheck]) {
        for c in checks {
            self.acknowledged.push(c.id.clone());
            if c.code.as_deref() == Some("irrelevant_domain")
                && let Some(site) = c.id.strip_prefix("kai_sc_irrelevant_domain_")
            {
                self.allowed.push(site.to_string());
            }
        }
    }
}

fn sensitive_list() -> &'static [String] {
    static LIST: OnceLock<Vec<String>> = OnceLock::new();
    LIST.get_or_init(|| {
        let user =
            std::fs::read_to_string(crate::settings::Settings::dir().join("sensitive_domains.txt"));
        parse_list(user.as_deref().unwrap_or(SENSITIVE_DEFAULT))
    })
}

fn parse_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_ascii_lowercase)
        .collect()
}

fn on_list(host: &str, list: &[String]) -> bool {
    list.iter()
        .any(|d| host == d || host.ends_with(&format!(".{d}")))
}

/// The checks for acting on `url` (the page, or where `open_url` goes),
/// minus those the user already accepted this run.
pub fn checks(url: Option<&str>, run: &RunDomains) -> Vec<SafetyCheck> {
    checks_with(url, run, sensitive_list())
}

fn checks_with(url: Option<&str>, run: &RunDomains, sensitive: &[String]) -> Vec<SafetyCheck> {
    let Some(host) = url.and_then(host_of) else {
        return Vec::new();
    };
    let site = registrable(&host);
    let mut out = Vec::new();
    if !run.named.is_empty() && !run.named.contains(&site) && !run.allowed.contains(&site) {
        out.push(SafetyCheck {
            id: format!("kai_sc_irrelevant_domain_{site}"),
            code: Some("irrelevant_domain".into()),
            message: Some(format!(
                "{site} is not one of the sites this task names ({}). Kai is about to act on it.",
                run.named.join(", ")
            )),
        });
    }
    if on_list(&host, sensitive) {
        out.push(SafetyCheck {
            id: format!("kai_sc_sensitive_domain_{host}"),
            code: Some("sensitive_domain".into()),
            message: Some(format!(
                "{host} holds sensitive accounts (mail, money, identity or health). Watch \
                 what Kai does here."
            )),
        });
    }
    out.retain(|c| !run.acknowledged.contains(&c.id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_sites() {
        assert_eq!(
            host_of("https://www.Example.com:8080/a?b").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            host_of("http://user@docs.github.com/x").as_deref(),
            Some("docs.github.com")
        );
        assert_eq!(host_of("about:blank"), None);
        assert_eq!(registrable("docs.github.com"), "github.com");
        assert_eq!(registrable("news.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(registrable("bmsce.ac.in"), "bmsce.ac.in");
    }

    #[test]
    fn sites_named_in_a_task() {
        assert_eq!(
            named_in(
                "Open bmsce.ac.in and find the IEEE CS page, e.g. via https://www.ieee.org/x."
            ),
            vec!["bmsce.ac.in", "ieee.org"]
        );
        assert!(named_in("Rename v1.2 of the report").is_empty());
    }

    #[test]
    fn an_unnamed_site_is_flagged_until_the_user_allows_it() {
        let mut run = RunDomains::new("Find the IEEE page on bmsce.ac.in");
        assert!(checks_with(Some("https://bmsce.ac.in/clubs"), &run, &[]).is_empty());
        let off = checks_with(Some("https://evil.example.net/login"), &run, &[]);
        assert_eq!(off.len(), 1);
        assert_eq!(off[0].code.as_deref(), Some("irrelevant_domain"));
        run.accept(&off);
        assert!(checks_with(Some("https://shop.example.net/"), &run, &[]).is_empty());
        // A task that names no site never raises this check.
        let open = RunDomains::new("Search the web for penguin facts");
        assert!(checks_with(Some("https://anything.org"), &open, &[]).is_empty());
    }

    #[test]
    fn sensitive_sites_are_flagged_even_when_named() {
        let list = parse_list(SENSITIVE_DEFAULT);
        let run = RunDomains::new("Read my newest email on mail.google.com");
        let c = checks_with(Some("https://mail.google.com/mail/u/0"), &run, &list);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].code.as_deref(), Some("sensitive_domain"));
        assert!(checks_with(Some("https://google.com/search"), &run, &list).is_empty());
        assert!(on_list("www.paypal.com", &list) || on_list("paypal.com", &list));
    }
}
