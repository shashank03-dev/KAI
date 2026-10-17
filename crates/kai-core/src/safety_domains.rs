//! Per-run domain trust, URL normalization, and observed link-hop grants.
use super::parse_list;
use crate::cua_protocol::SafetyCheck;
use std::sync::OnceLock;

/// The host of an HTTP(S) web address or a bare host. Never infer a domain
/// from arbitrary schemes or malformed authority components.
pub fn host_of(url: &str) -> Option<String> {
    let address = url.trim();
    if address.is_empty() || address.contains('\\') || address.chars().any(char::is_whitespace) {
        return None;
    }
    let full = if address.contains("://") {
        address.to_string()
    } else {
        format!("https://{address}")
    };
    let parsed = url::Url::parse(&full).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.trim_end_matches('.');
    let ip = host
        .trim_matches(['[', ']'])
        .parse::<std::net::IpAddr>()
        .is_ok();
    if (!host.contains('.') && !ip) || (!ip && host.split('.').any(str::is_empty)) {
        return None;
    }
    Some(
        host.strip_prefix("www.")
            .unwrap_or(host)
            .to_ascii_lowercase(),
    )
}

/// The registrable site including private PSL suffixes (`user.github.io`).
pub fn registrable(host: &str) -> String {
    psl::domain_str(host).unwrap_or(host).to_string()
}

/// Sites the task names, as `example.com` or a full address.
pub fn named_in(task: &str) -> Vec<String> {
    let mut sites: Vec<String> = task
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '"' | '\''))
        .map(|w| w.trim_end_matches(['.', '!', '?', ':']))
        // An email address is not a request to browse its mail provider.
        .filter(|w| !w.contains('@') || w.starts_with("https://") || w.starts_with("http://"))
        .filter(|w| {
            w.contains("://")
                || ![".md", ".rs", ".py", ".sh", ".txt", ".pdf", ".csv"]
                    .iter()
                    .any(|ext| w.ends_with(ext))
        })
        .filter(|w| w.contains('.') && w.chars().any(|c| c.is_ascii_alphabetic()))
        .filter_map(host_of)
        // Filenames and version numbers do not confer permission to visit a
        // lookalike host. Public/private suffixes both come from the PSL.
        .filter(|h| psl::suffix(h.as_bytes()).is_some_and(|s| s.is_known()))
        .map(|h| registrable(&h))
        .collect();
    sites.dedup();
    sites
}

/// What this run may visit without a check.
#[derive(Debug, Default, Clone)]
pub struct RunDomains {
    pub(super) named: Vec<String>,
    allowed: Vec<String>,
    pub(super) acknowledged: Vec<String>,
    linked: Vec<(String, u8)>,
    pending_link: Option<(String, u8)>,
}

impl RunDomains {
    pub fn new(task: &str) -> Self {
        Self {
            named: named_in(task),
            allowed: allowed_sites().to_vec(),
            ..Default::default()
        }
    }

    /// The person accepted these checks on the current URL.
    pub fn accept(&mut self, url: Option<&str>, checks: &[SafetyCheck]) {
        for c in checks {
            // A malformed origin has no stable identity to trust. Every
            // subsequent action there still needs its own safety review.
            if c.code.as_deref() == Some("irrelevant_domain")
                && url.is_some()
                && url.and_then(host_of).is_none()
            {
                continue;
            }
            if !self.acknowledged.contains(&c.id) {
                self.acknowledged.push(c.id.clone());
            }
            if c.code.as_deref() == Some("irrelevant_domain")
                && let Some(site) = url.and_then(host_of).map(|h| registrable(&h))
                && !self.allowed.contains(&site)
            {
                self.allowed.push(site);
            }
        }
    }

    fn depth(&self, site: &str) -> Option<u8> {
        if self.named.iter().chain(&self.allowed).any(|s| s == site) {
            Some(0)
        } else {
            self.linked.iter().find(|(s, _)| s == site).map(|(_, n)| *n)
        }
    }

    /// Called only after successfully clicking an observed accessibility link.
    pub fn link_clicked(&mut self, url: Option<&str>) {
        self.pending_link = url
            .and_then(host_of)
            .map(|h| registrable(&h))
            .and_then(|site| self.depth(&site).filter(|n| *n < 2).map(|n| (site, n)));
    }

    /// Observe where the link landed. One link may grant one extra hop.
    pub fn arrived(&mut self, url: Option<&str>) {
        let Some((from, depth)) = self.pending_link.clone() else {
            return;
        };
        let Some(site) = url.and_then(host_of).map(|h| registrable(&h)) else {
            self.pending_link = None;
            return;
        };
        // A link can load asynchronously; an unchanged source is not a landing.
        if site == from {
            return;
        }
        self.pending_link = None;
        if self.depth(&site).is_none() {
            self.linked.push((site, depth + 1));
        }
    }

    /// An unrelated action must not inherit a prior link's navigation grant.
    pub fn abandon_link(&mut self) {
        self.pending_link = None;
    }

    pub(super) fn is_allowed(&self, site: &str) -> bool {
        self.depth(site).is_some()
    }
}

/// Sites explicitly trusted by the user in their settings directory.
fn allowed_sites() -> &'static [String] {
    static LIST: OnceLock<Vec<String>> = OnceLock::new();
    LIST.get_or_init(|| {
        let text =
            std::fs::read_to_string(crate::settings::Settings::dir().join("allowed_domains.txt"))
                .unwrap_or_default();
        parse_allowed_sites(&text)
    })
}

pub(super) fn parse_allowed_sites(text: &str) -> Vec<String> {
    parse_list(text)
        .iter()
        .filter_map(|s| host_of(s))
        .filter(|s| psl::suffix(s.as_bytes()).is_some_and(|suffix| suffix.is_known()))
        .filter_map(|s| psl::domain_str(&s).map(str::to_string))
        .collect()
}

impl RunDomains {
    pub fn pending(&self, check: &SafetyCheck) -> bool {
        !self.acknowledged.contains(&check.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety_checks::checks;

    #[test]
    fn delayed_link_landing_does_not_consume_the_one_navigation_grant() {
        let mut run = RunDomains::new("Visit example.org");
        run.link_clicked(Some("https://example.org/page"));
        run.arrived(Some("https://example.org/page"));
        run.arrived(Some("https://project.github.io/"));
        assert!(checks(Some("https://project.github.io/"), &run).is_empty());
        assert_eq!(checks(Some("https://other.github.io/"), &run).len(), 1);
        run.link_clicked(Some("https://project.github.io/"));
        run.arrived(Some("https://docs.example.net/"));
        assert!(checks(Some("https://docs.example.net/"), &run).is_empty());
        run.link_clicked(Some("https://docs.example.net/"));
        run.arrived(Some("https://third.example.com/"));
        assert_eq!(checks(Some("https://third.example.com/"), &run).len(), 1);
    }

    #[test]
    fn unrelated_actions_revoke_the_link_grant() {
        let mut run = RunDomains::new("Visit example.org");
        run.link_clicked(Some("https://example.org/"));
        run.arrived(Some("https://example.org/"));
        run.abandon_link();
        run.arrived(Some("https://unrelated.example.net/"));
        assert_eq!(
            checks(Some("https://unrelated.example.net/"), &run).len(),
            1
        );
    }

    #[test]
    fn invalid_origin_never_becomes_trusted_after_approval() {
        let mut run = RunDomains::new("Visit example.org");
        for bad in [
            "https://example.org:bad",
            "https://evil.example.net\\@example.org",
        ] {
            let pending = checks(Some(bad), &run);
            assert_eq!(pending.len(), 1, "{bad}");
            assert_eq!(pending[0].code.as_deref(), Some("irrelevant_domain"));
            run.accept(Some(bad), &pending);
            assert_eq!(checks(Some(bad), &run).len(), 1);
        }
        assert_eq!(host_of("https://[::1]/"), Some("[::1]".into()));
        assert_eq!(checks(Some("https://[::1]/"), &run).len(), 1);
    }
}
