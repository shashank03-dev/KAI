//! [`BrowserLane`]: Kai's own Chromium, driven over the DevTools pipe. It is
//! a Perceptor and an Actuator, so the engine, the prompt, the schema, the
//! risk rules and the outcome ledger stay exactly as on the desktop; only the
//! eyes and hands change (`docs/BROWSER.md`).

use super::cdp::Cdp;
use super::dialog::DialogTarget;
use super::snapshot::{DomRef, PageInfo};
use kai_core::{Actuator, Grounding, Rect};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How to start the lane's browser.
#[derive(Debug, Clone)]
pub struct LaneOptions {
    /// A Chromium-family executable ([`find_browser`]).
    pub browser: PathBuf,
    /// Kai's own profile folder: the user signs in to sites here once. Never
    /// the user's everyday profile (Chrome refuses debugging there anyway).
    pub profile: PathBuf,
    /// No window (bench, tests, CI). The viewport is then `size`.
    pub headless: bool,
    /// Window (or headless viewport) size in CSS pixels.
    pub size: (u32, u32),
    /// Legacy download destination. Downloads are denied until Kai has an
    /// approval path for writing files outside its own profile.
    pub downloads: Option<PathBuf>,
}

/// Executable names tried on `PATH`, in order.
const BROWSER_NAMES: &[&str] = &[
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "microsoft-edge",
    "brave-browser",
];

/// Chromium-family app bundles on a Mac, in order (the executable inside
/// each, under `/Applications` or `~/Applications`).
const MAC_APPS: &[&str] = &[
    "Google Chrome.app/Contents/MacOS/Google Chrome",
    "Chromium.app/Contents/MacOS/Chromium",
    "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "Brave Browser.app/Contents/MacOS/Brave Browser",
];

/// Chromium-family installs on Windows, under `%ProgramFiles%`,
/// `%ProgramFiles(x86)%` or `%LOCALAPPDATA%` (they are not on `PATH`).
const WINDOWS_APPS: &[&str] = &[
    "Google/Chrome/Application/chrome.exe",
    "Microsoft/Edge/Application/msedge.exe",
    "Chromium/Application/chrome.exe",
    "BraveSoftware/Brave-Browser/Application/brave.exe",
];

/// The browser to drive: `KAI_BROWSER` if set, else the first Chromium-family
/// executable on `PATH`, else the first standard install (macOS app bundles,
/// Windows program folders).
pub fn find_browser() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KAI_BROWSER").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let exe = if cfg!(windows) { ".exe" } else { "" };
    let on_path = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .flat_map(|dir| {
                BROWSER_NAMES
                    .iter()
                    .map(move |n| dir.join(format!("{n}{exe}")))
            })
            .find(|p| p.is_file())
    });
    if cfg!(windows) && on_path.is_none() {
        return ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(std::env::var_os)
            .flat_map(|dir| {
                WINDOWS_APPS
                    .iter()
                    .map(move |a| PathBuf::from(&dir).join(a))
            })
            .find(|p| p.is_file());
    }
    on_path.or_else(|| {
        if !cfg!(target_os = "macos") {
            return None;
        }
        let home = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications"));
        [Some(PathBuf::from("/Applications")), home]
            .into_iter()
            .flatten()
            .flat_map(|dir| MAC_APPS.iter().map(move |a| dir.join(a)))
            .find(|p| p.is_file())
    })
}

/// What a mark stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MarkTarget {
    Dom(DomRef),
    Dialog(DialogTarget),
    /// An option of the dropdown the lane opened (`dropdown.rs`).
    Option {
        select: DomRef,
        index: usize,
    },
}

/// The marks of the latest observation and what each one stands for.
#[derive(Default)]
pub(super) struct Marked {
    pub grounding: Option<Grounding>,
    pub targets: Vec<MarkTarget>,
}

/// The tab Kai is working in.
#[derive(Debug, Clone)]
pub(super) struct PageRef {
    pub target: String,
    pub session: String,
    /// The tab that opened this one (`target=_blank`, `window.open`); it
    /// takes over again when this one closes.
    pub opener: Option<String>,
}

pub struct BrowserLane {
    pub(super) cdp: Cdp,
    pub(super) page: Mutex<PageRef>,
    pub(super) marked: Mutex<Marked>,
    /// Shell and app launches go to the desktop's hands when Kai has them.
    pub(super) fallback: Option<Box<dyn Actuator>>,
    pub(super) headless: bool,
    pub(super) size: (u32, u32),
    /// What the model typed into an open `prompt()` dialog.
    pub(super) answer: Mutex<Option<String>>,
    /// The page facts and viewport of the last successful read, shown while
    /// a dialog freezes the page.
    pub(super) last: Mutex<(PageInfo, Rect)>,
    /// A click opened a file picker (intercepted: Kai cannot choose files).
    pub(super) file_chooser: AtomicBool,
    pub(super) input_poisoned: AtomicBool,
    /// The marks of the last dialog shown, so an action on one of them after
    /// the dialog closed is refused instead of clicking the page under it.
    pub(super) last_dialog: Mutex<Vec<kai_core::Element>>,
    /// The native dropdown whose options are listed as marks.
    pub(super) open_select: Mutex<Option<DomRef>>,
    /// Exclusive ownership of this browser profile until the browser closes.
    /// Keep after `cdp` so the child exits before the lock is released.
    _profile_lock: std::fs::File,
}

impl BrowserLane {
    /// Start the browser on `about:blank` and attach to its page.
    pub async fn launch(
        opts: &LaneOptions,
        fallback: Option<Box<dyn Actuator>>,
    ) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&opts.profile)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&opts.profile, std::fs::Permissions::from_mode(0o700))?;
        }
        // Some Chromium builds (notably Playwright's headless shell) allow
        // simultaneous processes with the same profile. Require exclusive
        // ownership ourselves, including across separate Kai processes.
        // An OS-managed file lock is released on process exit or failed launch.
        let profile_lock = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(opts.profile.join(".kai-profile.lock"))?;
        match profile_lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                anyhow::bail!(
                    "browser profile {} is already in use by another Kai browser",
                    opts.profile.display()
                );
            }
            Err(e) => return Err(e.into()),
        }
        let cdp = Cdp::launch(&opts.browser, &args(opts))?;
        let targets = match cdp.call("Target.getTargets", json!({}), None).await {
            Ok(t) => t,
            Err(e) if exited(&cdp).await => anyhow::bail!(
                "the browser ({}) exited at start: {e}. Is another Kai browser already \
                 open with the profile {}?",
                opts.browser.display(),
                opts.profile.display()
            ),
            Err(e) => return Err(e),
        };
        cdp.call("Target.setDiscoverTargets", json!({"discover": true}), None)
            .await?;
        cdp.call(
            "Browser.setDownloadBehavior",
            json!({"behavior": "deny", "eventsEnabled": true}),
            None,
        )
        .await?;
        let page = targets["targetInfos"]
            .as_array()
            .and_then(|t| t.iter().find(|t| t["type"] == "page"))
            .and_then(|t| t["targetId"].as_str())
            .map(str::to_string);
        let target = match page {
            Some(id) => id,
            None => new_tab(&cdp).await?,
        };
        let session = attach(&cdp, &target, opts.headless, opts.size).await?;
        Ok(Self {
            cdp,
            page: Mutex::new(PageRef {
                target,
                session,
                opener: None,
            }),
            marked: Mutex::default(),
            fallback,
            headless: opts.headless,
            size: opts.size,
            answer: Mutex::default(),
            last: Mutex::new((
                PageInfo::default(),
                Rect::new(0.0, 0.0, opts.size.0 as f32, opts.size.1 as f32),
            )),
            file_chooser: AtomicBool::new(false),
            input_poisoned: AtomicBool::new(false),
            last_dialog: Mutex::default(),
            open_select: Mutex::default(),
            _profile_lock: profile_lock,
        })
    }

    /// Whether the browser is still running (the user may close it).
    pub fn is_open(&self) -> bool {
        self.cdp.alive()
    }

    /// A cancelled input left its release unacknowledged. Recreate the lane
    /// before another run attempts to observe or act through it.
    pub fn is_poisoned(&self) -> bool {
        self.input_poisoned.load(Ordering::SeqCst)
    }

    pub(super) fn current(&self) -> PageRef {
        self.page.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(super) async fn page(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        if let Ok(result) = super::computer::INPUT_CONTEXT.try_with(|ctx| ctx.check()) {
            result?;
        }
        anyhow::ensure!(
            !self.input_poisoned.load(Ordering::SeqCst),
            "browser input cleanup was not acknowledged; restart the browser"
        );
        if !self.cdp.alive() {
            anyhow::bail!("Kai's browser was closed");
        }
        let session = self.current().session;
        self.cdp.call(method, params, Some(&session)).await
    }

    /// The open dialog on the current tab, if any.
    pub(super) fn dialog(&self) -> Option<Value> {
        self.cdp.dialog(&self.current().session)
    }

    pub(super) async fn ready_state(&self) -> Option<String> {
        let r = self
            .page(
                "Runtime.evaluate",
                json!({"expression": "document.readyState", "returnByValue": true}),
            )
            .await
            .ok()?;
        r["result"]["value"].as_str().map(str::to_string)
    }

    /// Wait until the document has loaded, up to `max`. Returns early when
    /// a dialog freezes the page or the browser is gone.
    pub(super) async fn wait_loaded(&self, max: Duration) {
        let deadline = Instant::now() + max;
        while Instant::now() < deadline {
            if self.dialog().is_some() || !self.cdp.alive() {
                return;
            }
            if self.ready_state().await.as_deref() == Some("complete") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

/// Whether the browser process has ended, allowing it a moment: the pipe
/// closes just before the process is gone.
async fn exited(cdp: &Cdp) -> bool {
    for _ in 0..20 {
        if !cdp.alive() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// A new blank tab; its target id.
pub(super) async fn new_tab(cdp: &Cdp) -> anyhow::Result<String> {
    Ok(cdp
        .call("Target.createTarget", json!({"url": "about:blank"}), None)
        .await?["targetId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("the browser opened no page"))?
        .to_string())
}

/// Attach to a tab and turn on what the lane reads. File pickers are
/// intercepted so a native dialog can never block the browser.
pub(super) async fn attach(
    cdp: &Cdp,
    target: &str,
    headless: bool,
    size: (u32, u32),
) -> anyhow::Result<String> {
    let attached = cdp
        .call(
            "Target.attachToTarget",
            json!({"targetId": target, "flatten": true}),
            None,
        )
        .await?;
    let session = attached["sessionId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("could not attach to the browser page"))?
        .to_string();
    for m in ["Page.enable", "DOM.enable", "Accessibility.enable"] {
        cdp.call(m, json!({}), Some(&session)).await?;
    }
    cdp.call(
        "Page.setInterceptFileChooserDialog",
        json!({"enabled": true}),
        Some(&session),
    )
    .await?;
    if headless {
        let (w, h) = size;
        cdp.call(
            "Emulation.setDeviceMetricsOverride",
            json!({"width": w, "height": h, "deviceScaleFactor": 1, "mobile": false}),
            Some(&session),
        )
        .await?;
        // A headless page has focus only some of the time (Chrome 153: half
        // the runs), and the media completion check needs `hasFocus()`;
        // the working tab is the focused one, as in a visible window.
        cdp.call(
            "Emulation.setFocusEmulationEnabled",
            json!({"enabled": true}),
            Some(&session),
        )
        .await?;
    }
    Ok(session)
}

/// Command line for the lane's browser.
fn args(o: &LaneOptions) -> Vec<String> {
    let mut a = vec![
        format!("--user-data-dir={}", o.profile.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        // Keep rendering while covered or in the background, so screenshots
        // and input work while the user does something else.
        "--disable-backgrounding-occluded-windows".into(),
        "--disable-renderer-backgrounding".into(),
        "--disable-background-timer-throttling".into(),
        format!("--window-size={},{}", o.size.0, o.size.1),
    ];
    if o.headless {
        a.push("--headless=new".into());
    }
    #[cfg(target_os = "linux")]
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        // Chromium refuses to start as root with its sandbox on.
        log::warn!("running as root: starting the browser without its sandbox");
        a.push("--no-sandbox".into());
    }
    a.push("about:blank".into());
    a
}

/// The user's Downloads folder: `XDG_DOWNLOAD_DIR` when set, else
/// `~/Downloads` (`%USERPROFILE%\Downloads` on Windows).
pub fn downloads_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_DOWNLOAD_DIR").filter(|d| !d.is_empty()) {
        return Some(d.into());
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    Some(PathBuf::from(home).join("Downloads"))
}

/// Where the lane keeps its profile by default.
pub fn default_profile(config_dir: &Path) -> PathBuf {
    config_dir.join("browser")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn profile_is_private_even_when_it_already_exists() {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let tmp = tempfile::tempdir().unwrap();
        let profile = tmp.path().join("browser");
        std::fs::DirBuilder::new()
            .mode(0o755)
            .create(&profile)
            .unwrap();
        let opts = LaneOptions {
            browser: tmp.path().join("missing-chromium"),
            profile: profile.clone(),
            headless: true,
            size: (800, 600),
            downloads: None,
        };
        assert!(BrowserLane::launch(&opts, None).await.is_err());
        assert_eq!(
            std::fs::metadata(profile).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn root_or_not_the_browser_gets_kais_own_profile_and_no_debug_port() {
        let o = LaneOptions {
            browser: "chromium".into(),
            profile: "/tmp/kai-profile".into(),
            headless: true,
            size: (1280, 800),
            downloads: None,
        };
        let a = args(&o);
        assert!(a.contains(&"--user-data-dir=/tmp/kai-profile".to_string()));
        assert!(a.contains(&"--headless=new".to_string()));
        assert!(!a.iter().any(|x| x.starts_with("--remote-debugging-port")));
    }
}
