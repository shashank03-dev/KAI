//! Outcome probes: the fixed menu a required outcome may name, how the model
//! writes one, and the probes answered from the observation Kai already has
//! (`a11y_has`, `window_title`, `url_is`). The OS probes (`file_exists`,
//! `file_contains`, `shell_ro`) go through [`crate::ports::Probe`], which
//! `kai-os` implements. See `docs/VERIFICATION.md`.

use crate::element::{Grounding, Role};
use crate::ports::Observation;
use crate::shell_ro::ReadOnlyCmd;
use serde::Deserialize;

/// Longest path, text, name or pattern a probe may carry.
const MAX_ARG: usize = 200;

/// One probe from the menu. Regexes are compiled once, case-insensitive.
#[derive(Debug, Clone)]
pub enum ProbeSpec {
    FileExists { path: String },
    FileContains { path: String, text: String },
    A11yHas { role: String, name: String },
    WindowTitle { pattern: regex::Regex },
    UrlIs { pattern: regex::Regex },
    ShellRo { cmd: ReadOnlyCmd },
}

/// What a probe said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeVerdict {
    pub ok: bool,
    /// A few words: "not found", "exit 1", "title is \"…\"". Never raw
    /// command output.
    pub reason: String,
}

impl ProbeVerdict {
    pub fn pass(reason: impl Into<String>) -> Self {
        Self {
            ok: true,
            reason: reason.into(),
        }
    }
    pub fn fail(reason: impl Into<String>) -> Self {
        Self {
            ok: false,
            reason: reason.into(),
        }
    }
}

/// The probe as the model writes it: `{"kind":"file_contains","path":…,
/// "text":…}`. Unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawProbe {
    pub kind: String,
    pub path: Option<String>,
    pub text: Option<String>,
    pub role: Option<String>,
    pub name: Option<String>,
    pub pattern: Option<String>,
    pub cmd: Option<String>,
}

fn arg(v: &Option<String>, what: &str) -> Result<String, String> {
    let v = v.as_deref().map(str::trim).unwrap_or("");
    if v.is_empty() {
        return Err(format!("missing `{what}`"));
    }
    if v.chars().count() > MAX_ARG {
        return Err(format!("`{what}` longer than {MAX_ARG} characters"));
    }
    Ok(v.to_string())
}

fn regex(p: &str) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(p)
        .case_insensitive(true)
        .size_limit(1 << 16)
        .build()
        .map_err(|_| "invalid regex".to_string())
}

impl ProbeSpec {
    /// Check a probe against the menu. The error says why it was refused.
    pub fn parse(raw: &RawProbe) -> Result<Self, String> {
        Ok(match raw.kind.trim() {
            "file_exists" => Self::FileExists {
                path: arg(&raw.path, "path")?,
            },
            "file_contains" => Self::FileContains {
                path: arg(&raw.path, "path")?,
                text: arg(&raw.text, "text")?,
            },
            "a11y_has" => {
                let name = arg(&raw.name, "name")?;
                if name.chars().count() < 2 {
                    return Err("`name` shorter than 2 characters".into());
                }
                Self::A11yHas {
                    role: raw.role.as_deref().unwrap_or("any").trim().to_lowercase(),
                    name,
                }
            }
            "window_title" => Self::WindowTitle {
                pattern: regex(&arg(&raw.pattern, "pattern")?)?,
            },
            "url_is" => Self::UrlIs {
                pattern: regex(&arg(&raw.pattern, "pattern")?)?,
            },
            "shell_ro" => Self::ShellRo {
                cmd: ReadOnlyCmd::parse(&arg(&raw.cmd, "cmd")?)?,
            },
            "" => return Err("missing `kind`".into()),
            other => return Err(format!("`{other}` is not a probe on the menu")),
        })
    }

    /// `file_contains(~/a.txt, Hello)`, for records and the UI.
    pub fn label(&self) -> String {
        match self {
            Self::FileExists { path } => format!("file_exists({path})"),
            Self::FileContains { path, text } => format!("file_contains({path}, {text})"),
            Self::A11yHas { role, name } => format!("a11y_has({role}, {name})"),
            Self::WindowTitle { pattern } => format!("window_title({})", pattern.as_str()),
            Self::UrlIs { pattern } => format!("url_is({})", pattern.as_str()),
            Self::ShellRo { cmd } => format!("shell_ro({})", cmd.source()),
        }
    }

    /// Needs the OS ([`crate::ports::Probe`]); the others read the
    /// observation.
    pub fn needs_os(&self) -> bool {
        matches!(
            self,
            Self::FileExists { .. } | Self::FileContains { .. } | Self::ShellRo { .. }
        )
    }

    /// Answer an observation probe. `None` for OS probes.
    pub fn on_screen(&self, obs: &Observation) -> Option<ProbeVerdict> {
        Some(match self {
            Self::A11yHas { role, name } => a11y_has(&obs.grounding, role, name),
            Self::WindowTitle { pattern } => match obs.window_title.as_deref() {
                Some(t) if pattern.is_match(t) => ProbeVerdict::pass("title matches"),
                Some(t) => ProbeVerdict::fail(format!("title is \"{}\"", clip(t, 60))),
                None => ProbeVerdict::fail("window title unknown"),
            },
            Self::UrlIs { pattern } => {
                match obs.url.clone().or_else(|| address_bar(&obs.grounding)) {
                    Some(u) if pattern.is_match(&u) => ProbeVerdict::pass("address matches"),
                    Some(u) => ProbeVerdict::fail(format!("address is \"{}\"", clip(&u, 80))),
                    None => ProbeVerdict::fail("no address bar visible"),
                }
            }
            _ => return None,
        })
    }
}

fn role_matches(want: &str, have: Role) -> bool {
    match want {
        "" | "any" | "element" | "*" => true,
        "text" | "entry" | "field" | "textbox" | "edit" => have == Role::TextField,
        "dropdown" | "select" => have == Role::ComboBox,
        "menu_item" | "menu" => have == Role::MenuItem,
        "list_item" | "item" => have == Role::ListItem,
        w => have.label() == w,
    }
}

fn a11y_has(g: &Grounding, role: &str, name: &str) -> ProbeVerdict {
    let Grounding::Marks(els) = g else {
        return ProbeVerdict::fail("no accessibility tree (grid screen)");
    };
    let want = name.to_lowercase();
    let hit = els.iter().any(|e| {
        role_matches(role, e.role)
            && (e.name.to_lowercase().contains(&want)
                || (!e.secure
                    && e.value
                        .as_deref()
                        .is_some_and(|v| v.to_lowercase().contains(&want))))
    });
    if hit {
        ProbeVerdict::pass("element found")
    } else {
        ProbeVerdict::fail("no such element on screen")
    }
}

/// Names browsers give their address bar.
const ADDRESS_NAMES: &[&str] = &[
    "address",
    "url",
    "location",
    "search or enter",
    "search with",
];

/// The browser address bar's current value, if one is on screen.
pub fn address_bar(g: &Grounding) -> Option<String> {
    let Grounding::Marks(els) = g else {
        return None;
    };
    els.iter()
        .filter(|e| matches!(e.role, Role::TextField | Role::ComboBox) && !e.secure)
        .filter(|e| {
            let n = e.name.to_lowercase();
            ADDRESS_NAMES.iter().any(|a| n.contains(a))
        })
        .find_map(|e| e.value.clone().filter(|v| !v.trim().is_empty()))
}

pub(crate) fn clip(s: &str, max: usize) -> String {
    let s = s.trim().replace('\n', " ");
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Images;
    use crate::element::{Element, Rect};

    fn raw(json: &str) -> RawProbe {
        serde_json::from_str(json).unwrap()
    }
    fn el(role: Role, name: &str, value: Option<&str>) -> Element {
        Element {
            mark: 1,
            role,
            name: name.into(),
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
            enabled: true,
            secure: false,
            focused: false,
            value: value.map(Into::into),
        }
    }
    fn obs(els: Vec<Element>, title: Option<&str>) -> Observation {
        Observation {
            images: Images {
                screenshot_jpeg: Vec::new(),
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(els),
            screen_hash: 0,
            timing: Default::default(),
            screen: None,
            window_title: title.map(Into::into),
            url: None,
        }
    }

    #[test]
    fn the_menu_parses_and_anything_else_is_refused() {
        let p = ProbeSpec::parse(&raw(
            r#"{"kind":"file_contains","path":"~/a.txt","text":"Hi"}"#,
        ));
        assert_eq!(p.unwrap().label(), "file_contains(~/a.txt, Hi)");
        assert!(ProbeSpec::parse(&raw(r#"{"kind":"shell_ro","cmd":"rm -rf ~"}"#)).is_err());
        assert!(ProbeSpec::parse(&raw(r#"{"kind":"shell","cmd":"ls"}"#)).is_err());
        assert!(ProbeSpec::parse(&raw(r#"{"kind":"window_title","pattern":"(("}"#)).is_err());
        assert!(ProbeSpec::parse(&raw(r#"{"kind":"file_exists"}"#)).is_err());
        assert!(ProbeSpec::parse(&raw(r#"{"kind":"a11y_has","name":"x"}"#)).is_err());
        assert!(ProbeSpec::parse(&raw(r#"{}"#)).is_err());
        let s = ProbeSpec::parse(&raw(r#"{"kind":"shell_ro","cmd":"ls ~"}"#)).unwrap();
        assert!(s.needs_os());
    }

    #[test]
    fn observation_probes_read_the_screen() {
        let o = obs(
            vec![
                el(Role::Button, "Send message", None),
                el(
                    Role::TextField,
                    "Search or enter address",
                    Some("127.0.0.1:8765/thanks"),
                ),
            ],
            Some("Thank you — Mozilla Firefox"),
        );
        let p = |j: &str| ProbeSpec::parse(&raw(j)).unwrap().on_screen(&o).unwrap();
        assert!(p(r#"{"kind":"a11y_has","role":"button","name":"send"}"#).ok);
        assert!(!p(r#"{"kind":"a11y_has","role":"link","name":"send"}"#).ok);
        assert!(
            p(r#"{"kind":"a11y_has","name":"thanks"}"#).ok,
            "values count"
        );
        assert!(p(r#"{"kind":"window_title","pattern":"thank you"}"#).ok);
        let f = p(r#"{"kind":"window_title","pattern":"^contact"}"#);
        assert_eq!(f.reason, "title is \"Thank you — Mozilla Firefox\"");
        assert!(p(r#"{"kind":"url_is","pattern":"/thanks$"}"#).ok);
        assert!(!p(r#"{"kind":"url_is","pattern":"/contact"}"#).ok);
        // The browser lane knows the real address; it wins over the bar.
        let mut lane = obs(vec![], None);
        lane.url = Some("https://shop.test/order/done".into());
        let u = |j: &str| ProbeSpec::parse(&raw(j)).unwrap().on_screen(&lane).unwrap();
        assert!(u(r#"{"kind":"url_is","pattern":"/order/done$"}"#).ok);
        assert!(!u(r#"{"kind":"url_is","pattern":"/cart"}"#).ok);
        let none = obs(vec![], None);
        let t = ProbeSpec::parse(&raw(r#"{"kind":"window_title","pattern":"x"}"#)).unwrap();
        assert_eq!(t.on_screen(&none).unwrap().reason, "window title unknown");
        let f = ProbeSpec::parse(&raw(r#"{"kind":"file_exists","path":"/x"}"#)).unwrap();
        assert!(f.on_screen(&none).is_none());
    }
}
