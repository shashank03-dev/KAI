//! `kai bench --browser`: each task runs in a fresh browser lane (its own
//! throwaway profile, so no cookies or tabs leak between tasks). Setup
//! scripts see `KAI_BENCH_LANE=1`; `open-browser.sh` then writes its URL to
//! `KAI_BENCH_OPEN_FILE` instead of starting a desktop browser, and the
//! lane opens it before the model's first step.

use kai_os::web::{BrowserLane, LaneOptions, find_browser};
use std::path::PathBuf;

pub struct BenchLane {
    pub lane: BrowserLane,
    open_file: PathBuf,
    _dir: tempfile::TempDir,
}

impl BenchLane {
    /// A fresh lane; shell and app launches go to the desktop's hands.
    pub async fn start(headless: bool, kill: &kai_os::KillSwitch) -> anyhow::Result<Self> {
        let browser = find_browser().ok_or_else(|| {
            anyhow::anyhow!("--browser needs Chrome, Chromium, Edge or Brave (or KAI_BROWSER)")
        })?;
        let dir = tempfile::tempdir()?;
        let desktop = kai_os::DesktopActuator::new(kill.clone())
            .map_err(|e| log::warn!("browser lane without desktop hands: {e}"))
            .ok()
            .map(|d| Box::new(d) as Box<dyn kai_core::Actuator>);
        let opts = LaneOptions {
            browser,
            profile: dir.path().join("profile"),
            headless,
            size: (1280, 800),
            downloads: std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")),
        };
        let lane = BrowserLane::launch(&opts, desktop).await?;
        Ok(Self {
            lane,
            open_file: dir.path().join("open-url"),
            _dir: dir,
        })
    }

    /// Variables for setup, check and teardown.
    pub fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("KAI_BENCH_LANE", "1".into()),
            ("KAI_BENCH_OPEN_FILE", self.open_file.display().to_string()),
        ]
    }

    /// Open the page the task's setup asked for, if any.
    pub async fn open_requested(&self) -> anyhow::Result<()> {
        let Ok(url) = std::fs::read_to_string(&self.open_file) else {
            return Ok(());
        };
        let url = url.trim();
        if url.is_empty() {
            return Ok(());
        }
        kai_core::Actuator::open_url(&self.lane, url).await
    }
}

/// Whether `--browser` can run here (the bench's `browser` need).
pub fn available() -> bool {
    find_browser().is_some()
}
