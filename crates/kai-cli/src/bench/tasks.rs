//! `bench/tasks.yaml`: the task list, and what each task needs on this machine.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// A command per OS. Missing means "not available on this OS".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerOs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<String>,
}

impl PerOs {
    pub fn is_empty(&self) -> bool {
        self.linux.is_none() && self.windows.is_none()
    }
}

impl PerOs {
    pub fn here(&self) -> Option<&str> {
        if cfg!(windows) {
            self.windows.as_deref()
        } else {
            self.linux.as_deref()
        }
    }
}

/// How a person would normally do the task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Terminal,
    Gui,
    Mixed,
}

/// What `--backend replay` proves for this task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    /// The recorded replies do the task the way the prompt means, GUI included.
    #[default]
    Real,
    /// The replies reach the same end state from the terminal: replay proves
    /// setup and check, not GUI skill.
    Terminal,
    /// Not replayable (needs the internet, or marks unknown in advance).
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub prompt: String,
    #[serde(default)]
    pub kind: Kind,
    /// Things that must exist here, or the task is skipped (see [`Need`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    #[serde(default, skip_serializing_if = "PerOs::is_empty")]
    pub setup: PerOs,
    /// Exit code 0 means pass. Without a check, the run's own outcome decides.
    #[serde(default, skip_serializing_if = "PerOs::is_empty")]
    pub check: PerOs,
    #[serde(default, skip_serializing_if = "PerOs::is_empty")]
    pub teardown: PerOs,
    /// Recorded replies for the mock and replay backends.
    #[serde(default)]
    pub mock: Vec<String>,
    #[serde(default)]
    pub replay: ReplayMode,
    /// Values for `{name}` placeholders in title/prompt/setup/check/teardown
    /// and the recorded replies. The first value of each is the default the
    /// bench runs; `kai bench variants` renders the others (training data).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, Vec<String>>,
    /// Set on generated variants: the id of the task they were made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_of: Option<String>,
}

/// Every `needs` value the bench understands.
pub const KNOWN_NEEDS: &[&str] = &[
    "browser",
    "internet",
    "server",
    "display",
    "mousepad",
    "pcmanfm",
    "libreoffice",
    "imagemagick",
];

pub fn load_tasks(path: &Path) -> anyhow::Result<Vec<Task>> {
    parse_tasks(&std::fs::read_to_string(path)?)
        .map_err(|e| e.context(format!("in {}", path.display())))
}

/// Parse and validate, with every `{var}` rendered to its default (first)
/// value, so tasks with `vars:` run exactly as written before.
pub fn parse_tasks(text: &str) -> anyhow::Result<Vec<Task>> {
    let raw = parse_raw(text)?;
    let tasks: Vec<Task> = raw.iter().map(super::variants::render_default).collect();
    validate(&tasks)?;
    Ok(tasks)
}

/// Parse with placeholders left in place (for `kai bench variants`).
pub fn parse_raw(text: &str) -> anyhow::Result<Vec<Task>> {
    let tasks: Vec<Task> = serde_yaml::from_str(text)?;
    for t in &tasks {
        super::variants::check_vars(t)?;
    }
    Ok(tasks)
}

/// The checks every task list must pass (after rendering).
pub fn validate(tasks: &[Task]) -> anyhow::Result<()> {
    anyhow::ensure!(!tasks.is_empty(), "no tasks");
    let mut seen = std::collections::HashSet::new();
    for t in tasks {
        anyhow::ensure!(seen.insert(t.id.as_str()), "duplicate task id {}", t.id);
        anyhow::ensure!(!t.mock.is_empty(), "task {} has no recorded replies", t.id);
        for n in &t.needs {
            anyhow::ensure!(
                KNOWN_NEEDS.contains(&n.as_str()),
                "task {}: unknown need {n:?} (known: {})",
                t.id,
                KNOWN_NEEDS.join(", ")
            );
        }
        for raw in &t.mock {
            kai_core::Decision::parse_checked(raw)
                .map_err(|e| anyhow::anyhow!("task {}: bad recorded reply {raw}: {e}", t.id))?;
        }
    }
    Ok(())
}

/// What this machine has, probed once per run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Available {
    pub have: Vec<String>,
}

impl Available {
    pub fn probe() -> Self {
        let have = KNOWN_NEEDS
            .iter()
            .filter(|n| probe_need(n))
            .map(|n| n.to_string())
            .collect();
        Self { have }
    }

    /// The needs of `task` this machine lacks.
    pub fn missing(&self, task: &Task) -> Vec<String> {
        task.needs
            .iter()
            .filter(|n| !self.have.contains(n))
            .cloned()
            .collect()
    }
}

fn on_path(cmd: &str) -> bool {
    let exe = if cfg!(windows) {
        format!("{cmd}.exe")
    } else {
        cmd.to_string()
    };
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(&exe).is_file()))
        .unwrap_or(false)
}

/// The Firefox (or Chromium) binary the browser tasks would use, if any.
pub fn browser_path() -> Option<String> {
    for b in ["firefox", "chromium", "chromium-browser", "google-chrome"] {
        if on_path(b) {
            return Some(b.to_string());
        }
    }
    let opt = Path::new("/opt/firefox/firefox");
    opt.is_file().then(|| opt.display().to_string())
}

fn probe_need(need: &str) -> bool {
    match need {
        // Windows always ships a browser and a desktop session.
        "browser" => cfg!(windows) || browser_path().is_some(),
        "display" => {
            cfg!(windows)
                || std::env::var_os("DISPLAY").is_some_and(|d| !d.is_empty())
                || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty())
        }
        "server" => on_path("python3") && (cfg!(windows) || on_path("curl")),
        "internet" => internet(),
        "mousepad" => on_path("mousepad"),
        "pcmanfm" => on_path("pcmanfm") && on_path("wmctrl"),
        "libreoffice" => on_path("soffice") && on_path("unzip"),
        "imagemagick" => on_path("convert") && on_path("identify"),
        _ => false,
    }
}

/// A quick TCP connect to a well-known host (through no proxy: the browser
/// tasks need the real thing).
fn internet() -> bool {
    use std::net::ToSocketAddrs;
    let Ok(mut addrs) = ("www.google.com", 443).to_socket_addrs() else {
        return false;
    };
    addrs.any(|a| {
        std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_secs(2)).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
- id: a
  title: A
  prompt: do a
  kind: gui
  needs: [display, mousepad]
  replay: terminal
  setup:
    linux: echo setup
  check:
    linux: "true"
    windows: exit 0
  mock:
    - '{"thought":"x","action":"done","risk":"none"}'
- id: b
  title: B
  prompt: do b
  mock:
    - '{"thought":"x","action":"shell","text":"ls","risk":"low"}'
"#;

    #[test]
    fn parses_kinds_needs_and_defaults() {
        let t = parse_tasks(GOOD).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].kind, Kind::Gui);
        assert_eq!(t[0].replay, ReplayMode::Terminal);
        assert_eq!(t[0].needs, vec!["display", "mousepad"]);
        assert_eq!(t[1].kind, Kind::Terminal);
        assert_eq!(t[1].replay, ReplayMode::Real);
        assert!(t[1].setup.here().is_none());
        if !cfg!(windows) {
            assert_eq!(t[0].check.here(), Some("true"));
        }
    }

    #[test]
    fn rejects_duplicates_unknown_needs_and_bad_replies() {
        let dup = format!("{GOOD}{}", &GOOD[GOOD.find("- id: b").unwrap()..]);
        assert!(
            parse_tasks(&dup)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        let need = GOOD.replace("[display, mousepad]", "[telepathy]");
        assert!(
            parse_tasks(&need)
                .unwrap_err()
                .to_string()
                .contains("unknown need")
        );
        let bad = GOOD.replace(r#""action":"shell","text":"ls""#, r#""action":"shell""#);
        assert!(parse_tasks(&bad).is_err());
        assert!(parse_tasks("[]").is_err());
    }

    #[test]
    fn missing_lists_only_absent_needs() {
        let t = parse_tasks(GOOD).unwrap();
        let av = Available {
            have: vec!["display".into()],
        };
        assert_eq!(av.missing(&t[0]), vec!["mousepad"]);
        assert!(av.missing(&t[1]).is_empty());
    }

    #[test]
    fn the_shipped_task_list_is_valid() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/tasks.yaml");
        let tasks = load_tasks(&path).unwrap();
        assert!(tasks.len() >= 25, "{} tasks", tasks.len());
        for id in [
            "search-web",
            "write-note",
            "rename-files",
            "dark-mode",
            "disk-space",
            "find-largest",
            "open-site-find",
            "play-video",
            "fill-form",
            "pc-checkup",
        ] {
            assert!(tasks.iter().any(|t| t.id == id), "lost task {id}");
        }
        // Every replayable task with a check must have one for Linux.
        for t in &tasks {
            if t.replay != ReplayMode::Skip && t.id != "pc-checkup" {
                assert!(t.check.linux.is_some(), "{} has no Linux check", t.id);
            }
        }
    }
}
