//! What the bench ran on, written into every results file.

use crate::ab::Ab;
use serde::{Deserialize, Serialize};

/// The A/B switches a run used, and whether this build could honour them.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AbRecord {
    #[serde(flatten)]
    pub flags: Ab,
    /// False: this build's engine has no such switches, so features ran with
    /// the engine's defaults whatever the flags say.
    pub applied: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Machine {
    pub cpu: String,
    pub cores: usize,
    pub os: String,
    pub kernel: String,
    /// e.g. "x11 :99 1920x1080" or "windows".
    pub display: String,
    /// Accessibility toolkit env as seen by apps started from here.
    pub a11y_env: String,
    pub browser: Option<String>,
    pub kai_version: String,
    pub git_commit: Option<String>,
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn cmd(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

impl Machine {
    pub fn probe() -> Self {
        let cpu = read("/proc/cpuinfo")
            .and_then(|t| {
                t.lines()
                    .find(|l| l.starts_with("model name"))
                    .and_then(|l| l.split_once(':'))
                    .map(|(_, v)| v.trim().to_string())
            })
            .or_else(|| env("PROCESSOR_IDENTIFIER"))
            .unwrap_or_else(|| "unknown".into());
        let os = read("/etc/os-release")
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                    .map(|v| v.trim_matches('"').to_string())
            })
            .unwrap_or_else(|| std::env::consts::OS.to_string());
        let kernel = read("/proc/sys/kernel/osrelease")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let display = if cfg!(windows) {
            "windows".to_string()
        } else {
            let session = env("XDG_SESSION_TYPE").unwrap_or_else(|| {
                if env("WAYLAND_DISPLAY").is_some() {
                    "wayland".into()
                } else {
                    "x11".into()
                }
            });
            let name = env("DISPLAY")
                .or_else(|| env("WAYLAND_DISPLAY"))
                .unwrap_or_else(|| "none".into());
            let geom = cmd("xdotool", &["getdisplaygeometry"])
                .map(|g| g.replace(' ', "x"))
                .unwrap_or_default();
            format!("{session} {name} {geom}").trim().to_string()
        };
        let a11y_env = ["GTK_MODULES", "ACCESSIBILITY_ENABLED", "QT_ACCESSIBILITY"]
            .iter()
            .filter_map(|k| env(k).map(|v| format!("{k}={v}")))
            .collect::<Vec<_>>()
            .join(" ");
        Self {
            cpu,
            cores: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(0),
            os,
            kernel,
            display,
            a11y_env,
            browser: super::tasks::browser_path(),
            kai_version: env!("CARGO_PKG_VERSION").to_string(),
            git_commit: cmd("git", &["rev-parse", "--short", "HEAD"]),
        }
    }
}

/// The command line as typed, quoted where needed. Keys never appear here:
/// they come from the keychain or the environment, not from arguments.
pub fn command_line() -> String {
    std::env::args()
        .enumerate()
        .map(|(i, a)| {
            let a = if i == 0 { "kai".to_string() } else { a };
            if a.is_empty() || a.contains(|c: char| c.is_whitespace() || "\"'$`\\".contains(c)) {
                format!("'{}'", a.replace('\'', r"'\''"))
            } else {
                a
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
