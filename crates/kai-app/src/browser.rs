//! Settings › "Do tasks in Kai's own browser" (docs/BROWSER.md). The app
//! keeps one browser lane for its whole session: started on the first run
//! that wants it, reused by the next runs (so a page Kai opened, a video it
//! started, stays on screen when the run ends), started again if the user
//! closed its window. Shell and app launches inside a browser run go to the
//! desktop's hands.

use kai_core::settings::Settings;
use kai_os::DesktopActuator;
use kai_os::web::{BrowserLane, LaneOptions, default_profile, downloads_dir, find_browser};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct Lanes(Mutex<Option<Arc<BrowserLane>>>);

impl Lanes {
    /// The open lane, or a new one.
    pub async fn get(&self, desktop: Arc<DesktopActuator>) -> anyhow::Result<Arc<BrowserLane>> {
        let mut slot = self.0.lock().await;
        if let Some(lane) = slot.as_ref().filter(|l| l.is_open()) {
            return Ok(lane.clone());
        }
        let browser = find_browser().ok_or_else(|| {
            anyhow::anyhow!(
                "Kai's browser needs Chrome, Chromium, Edge or Brave on this computer \
                 (or set KAI_BROWSER to one). Turn it off in Settings to use the desktop."
            )
        })?;
        let opts = LaneOptions {
            browser,
            profile: default_profile(&Settings::dir()),
            headless: false,
            size: (1280, 800),
            downloads: downloads_dir(),
        };
        let fallback: Box<dyn kai_core::Actuator> = Box::new(desktop);
        let lane = Arc::new(BrowserLane::launch(&opts, Some(fallback)).await?);
        *slot = Some(lane.clone());
        Ok(lane)
    }
}
