//! `kai run` helpers: the browser lane switch (`--browser`, docs/BROWSER.md)
//! and the training-data recorder (`--record`).

use crate::backend::{self, BackendArg};
use crate::bench;
use kai_core::settings::Settings;
use std::path::Path;

/// Where `kai run` acts.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct LaneArgs {
    /// Do the task in Kai's own browser over the DevTools Protocol instead
    /// of on the desktop: the page's own tree, background input, the real
    /// page address for checks. Needs Chrome, Chromium, Edge or Brave
    /// (`KAI_BROWSER` to pick one). Linux and macOS for now.
    #[arg(long)]
    pub browser: bool,
    /// With --browser: no window (servers, CI).
    #[arg(long, requires = "browser")]
    pub headless: bool,
}

impl LaneArgs {
    /// Start the lane when asked. Shell and app launches inside a browser
    /// run go to the desktop's hands when this machine has a desktop.
    pub async fn open(
        &self,
        kill: &kai_os::KillSwitch,
    ) -> anyhow::Result<Option<kai_os::web::BrowserLane>> {
        if !self.browser {
            return Ok(None);
        }
        let browser = kai_os::web::find_browser().ok_or_else(|| {
            anyhow::anyhow!(
                "no Chrome, Chromium, Edge or Brave found on PATH; set KAI_BROWSER to its path"
            )
        })?;
        let desktop = kai_os::DesktopActuator::new(kill.clone())
            .map_err(|e| log::warn!("browser-only run (no desktop hands: {e})"))
            .ok()
            .map(|d| Box::new(d) as Box<dyn kai_core::Actuator>);
        let opts = kai_os::web::LaneOptions {
            browser,
            profile: kai_os::web::default_profile(&Settings::dir()),
            headless: self.headless,
            size: (1280, 800),
            downloads: kai_os::web::downloads_dir(),
        };
        eprintln!(
            "Working in Kai's browser ({}); its profile is {}",
            opts.browser.display(),
            opts.profile.display()
        );
        Ok(Some(
            kai_os::web::BrowserLane::launch(&opts, desktop).await?,
        ))
    }
}

/// A recorder for `kai run --record <root>`: `<root>/<time>-<task slug>/`.
pub fn run_recorder(
    root: &Path,
    task: &str,
    b: BackendArg,
    s: &Settings,
) -> anyhow::Result<kai_core::TraceRecorder> {
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    let slug: String = bench::report::slug(task).chars().take(40).collect();
    let run_id = format!("{stamp}-{}", slug.trim_end_matches('-'));
    let (backend, model) = match (b, s.backend) {
        (BackendArg::Auto, kai_core::settings::BackendChoice::Both) => {
            ("auto-chain".to_string(), "auto-chain".to_string())
        }
        (BackendArg::Auto, choice) => (
            format!("{choice:?}").to_lowercase(),
            backend::model_label(b, s),
        ),
        _ => (b.name().to_string(), backend::model_label(b, s)),
    };
    let mut meta = kai_core::RunMeta::new(&run_id, task, "run", backend, model);
    meta.kai_version = bench::record::kai_version();
    let mut rec = kai_core::TraceRecorder::new(root.join(&run_id), meta)?;
    if s.careful_model != s.fast_model && !s.careful_model.is_empty() {
        rec = rec.with_careful_model(&s.careful_model);
    }
    Ok(rec)
}

/// `--backend local`: the model's server, started for this run if needed.
pub async fn local_server(
    b: BackendArg,
    s: &mut Settings,
) -> anyhow::Result<Option<kai_os::local::LocalServer>> {
    if b == BackendArg::Local
        || (b == BackendArg::Auto && s.backend == kai_core::settings::BackendChoice::Local)
    {
        crate::models::ensure_local(s).await
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kai_core::settings::BackendChoice;

    #[test]
    fn recorder_names_the_model_that_will_answer() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.backend = BackendChoice::Local;
        s.local_model = "holo-3.1-4b-q4".into();
        let local = run_recorder(dir.path(), "local task", BackendArg::Local, &s)
            .unwrap()
            .meta();
        assert_eq!(local.model, "holo-3.1-4b-q4");
        assert_eq!(local.backend, "local");
        let auto = run_recorder(dir.path(), "auto task", BackendArg::Auto, &s)
            .unwrap()
            .meta();
        assert_eq!(auto.model, "holo-3.1-4b-q4");
        assert_eq!(auto.backend, "local");
        s.backend = BackendChoice::Both;
        let chain = run_recorder(dir.path(), "chain task", BackendArg::Auto, &s)
            .unwrap()
            .meta();
        assert_eq!(chain.backend, "auto-chain");
        assert_eq!(chain.model, "auto-chain");
    }
}
