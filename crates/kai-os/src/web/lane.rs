//! [`BrowserLane`]: Kai's own Chromium, driven over the DevTools pipe. It is
//! a Perceptor and an Actuator, so the engine, the prompt, the schema, the
//! risk rules and the outcome ledger stay exactly as on the desktop; only the
//! eyes and hands change (`docs/BROWSER.md`).

use super::cdp::Cdp;
use super::snapshot::{self, PageInfo};
use crate::{a11y, frame, marks, zoom};
use async_trait::async_trait;
use kai_core::{Actuator, Grounding, Images, Observation, ObserveTiming, Perceptor, Rect};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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

/// The browser to drive: `KAI_BROWSER` if set, else the first Chromium-family
/// executable on `PATH`.
pub fn find_browser() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KAI_BROWSER").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| BROWSER_NAMES.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

/// The marks of the latest observation and the DOM node behind each one.
#[derive(Default)]
pub(super) struct Marked {
    pub grounding: Option<Grounding>,
    pub nodes: Vec<i64>,
}

pub struct BrowserLane {
    pub(super) cdp: Cdp,
    pub(super) session: String,
    pub(super) marked: Mutex<Marked>,
    /// Shell and app launches go to the desktop's hands when Kai has them.
    pub(super) fallback: Option<Box<dyn Actuator>>,
}

impl BrowserLane {
    /// Start the browser on `about:blank` and attach to its page.
    pub async fn launch(
        opts: &LaneOptions,
        fallback: Option<Box<dyn Actuator>>,
    ) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&opts.profile)?;
        let cdp = Cdp::launch(&opts.browser, &args(opts))?;
        let targets = cdp.call("Target.getTargets", json!({}), None).await?;
        let page = targets["targetInfos"]
            .as_array()
            .and_then(|t| t.iter().find(|t| t["type"] == "page"))
            .and_then(|t| t["targetId"].as_str())
            .map(str::to_string);
        let target = match page {
            Some(id) => id,
            None => cdp
                .call("Target.createTarget", json!({"url": "about:blank"}), None)
                .await?["targetId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("the browser opened no page"))?
                .to_string(),
        };
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
        if opts.headless {
            let (w, h) = opts.size;
            cdp.call(
                "Emulation.setDeviceMetricsOverride",
                json!({"width": w, "height": h, "deviceScaleFactor": 1, "mobile": false}),
                Some(&session),
            )
            .await?;
        }
        Ok(Self {
            cdp,
            session,
            marked: Mutex::default(),
            fallback,
        })
    }

    pub(super) async fn page(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.cdp.call(method, params, Some(&self.session)).await
    }

    /// The visible area in CSS pixels.
    async fn viewport(&self) -> anyhow::Result<Rect> {
        let m = self.page("Page.getLayoutMetrics", json!({})).await?;
        let v = &m["cssVisualViewport"];
        let f = |k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
        Ok(Rect::new(0.0, 0.0, f("clientWidth"), f("clientHeight")))
    }

    /// Tree + snapshot + viewport, in parallel; grounds and remembers marks.
    async fn read(&self) -> anyhow::Result<(Grounding, PageInfo, Rect, u64)> {
        let t = Instant::now();
        let (ax, snap, view) = tokio::join!(
            self.page("Accessibility.getFullAXTree", json!({})),
            self.page(
                "DOMSnapshot.captureSnapshot",
                json!({"computedStyles": ["visibility"]})
            ),
            self.viewport(),
        );
        let view = view?;
        let (nodes, info) = snapshot::nodes(&ax?, &snap?, view);
        let raw: Vec<_> = nodes.iter().map(|(n, _)| n.clone()).collect();
        let (grounding, picked) = a11y::ground_indexed(&raw, view);
        let ids = picked.iter().map(|&i| nodes[i].1).collect();
        if let Ok(mut m) = self.marked.lock() {
            *m = Marked {
                grounding: Some(grounding.clone()),
                nodes: ids,
            };
        }
        Ok((grounding, info, view, t.elapsed().as_millis() as u64))
    }

    /// The viewport as an image (works with the window covered or headless).
    async fn shot(&self) -> anyhow::Result<image::RgbaImage> {
        let r = self
            .page(
                "Page.captureScreenshot",
                json!({"format": "jpeg", "quality": 90}),
            )
            .await?;
        let b64 = r["data"].as_str().unwrap_or("");
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)?;
        Ok(image::load_from_memory(&bytes)?.to_rgba8())
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

    /// Wait until the document has loaded, up to `max`.
    pub(super) async fn wait_loaded(&self, max: Duration) {
        let deadline = Instant::now() + max;
        while Instant::now() < deadline {
            if self.ready_state().await.as_deref() == Some("complete") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn observation(
        grounding: Grounding,
        info: PageInfo,
        view: Rect,
        images: Images,
        hash: u64,
        timing: ObserveTiming,
    ) -> Observation {
        Observation {
            images,
            grounding,
            screen_hash: hash,
            timing,
            screen: Some(view),
            window_title: Some(info.title).filter(|t| !t.is_empty()),
            url: Some(info.url).filter(|u| !u.is_empty()),
        }
    }
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

/// Scale CSS-pixel marks into screenshot pixels for drawing.
fn scaled(g: &Grounding, k: f32, img: (u32, u32)) -> Grounding {
    match g {
        Grounding::Marks(els) => Grounding::Marks(
            els.iter()
                .map(|e| {
                    let mut e = e.clone();
                    let b = e.bounds;
                    e.bounds = Rect::new(b.x * k, b.y * k, b.w * k, b.h * k);
                    e
                })
                .collect(),
        ),
        Grounding::Grid { cols, rows, .. } => Grounding::Grid {
            cols: *cols,
            rows: *rows,
            screen: Rect::new(0.0, 0.0, img.0 as f32, img.1 as f32),
        },
    }
}

#[async_trait]
impl Perceptor for BrowserLane {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let t = Instant::now();
        let (read, shot) = tokio::join!(self.read(), self.shot());
        let (grounding, info, view, tree_ms) = read?;
        let img = shot?;
        let capture_ms = t.elapsed().as_millis() as u64;
        let t_enc = Instant::now();
        let k = if view.w > 0.0 {
            img.width() as f32 / view.w
        } else {
            1.0
        };
        let draw = scaled(&grounding, k, img.dimensions());
        let (jpeg, hash) = tokio::task::spawn_blocking(move || {
            let f = frame::downscale(&img);
            let hash = frame::content_hash(&f.image);
            let mut marked = f.image;
            marks::draw(&mut marked, &draw, f.scale);
            (frame::encode_jpeg(&marked), hash)
        })
        .await?;
        let images = Images {
            screenshot_jpeg: jpeg,
            crop_jpeg: None,
        };
        let timing = ObserveTiming {
            capture_ms,
            tree_ms,
            encode_ms: t_enc.elapsed().as_millis() as u64,
        };
        Ok(Self::observation(
            grounding, info, view, images, hash, timing,
        ))
    }

    /// The page's tree alone: no screenshot. Falls back to a full look when
    /// the page has no usable tree (canvas apps), as on the desktop.
    async fn observe_tree_only(&self) -> anyhow::Result<Observation> {
        let (grounding, info, view, tree_ms) = self.read().await?;
        if matches!(grounding, Grounding::Grid { .. }) {
            return self.observe().await;
        }
        let hash = crate::perceive::tree_hash(&grounding);
        let images = Images {
            screenshot_jpeg: Vec::new(),
            crop_jpeg: None,
        };
        let timing = ObserveTiming {
            capture_ms: 0,
            tree_ms,
            encode_ms: 0,
        };
        Ok(Self::observation(
            grounding, info, view, images, hash, timing,
        ))
    }

    async fn zoom(&self, rect: Rect) -> Option<Vec<u8>> {
        let view = self.viewport().await.ok()?;
        let img = self.shot().await.ok()?;
        let k = if view.w > 0.0 {
            img.width() as f32 / view.w
        } else {
            1.0
        };
        let r = Rect::new(rect.x * k, rect.y * k, rect.w * k, rect.h * k);
        let at = Rect::new(0.0, 0.0, img.width() as f32, img.height() as f32);
        let plan = zoom::plan(r, at)?;
        Some(frame::encode_jpeg(&zoom::render(&img, at, &plan)))
    }

    /// Settled when the document has loaded and two tree reads agree.
    async fn settle(&self, max: Duration) {
        let deadline = Instant::now() + max;
        tokio::time::sleep(Duration::from_millis(60)).await;
        self.wait_loaded(max).await;
        let mut last = None;
        while Instant::now() < deadline {
            let now = self
                .read()
                .await
                .ok()
                .map(|r| crate::perceive::tree_hash(&r.0));
            if now.is_some() && now == last {
                return;
            }
            last = now;
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
    }
}

/// Where the lane keeps its profile by default.
pub fn default_profile(config_dir: &Path) -> PathBuf {
    config_dir.join("browser")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_or_not_the_browser_gets_kais_own_profile_and_no_debug_port() {
        let o = LaneOptions {
            browser: "chromium".into(),
            profile: "/tmp/kai-profile".into(),
            headless: true,
            size: (1280, 800),
        };
        let a = args(&o);
        assert!(a.contains(&"--user-data-dir=/tmp/kai-profile".to_string()));
        assert!(a.contains(&"--headless=new".to_string()));
        assert!(!a.iter().any(|x| x.starts_with("--remote-debugging-port")));
    }

    #[test]
    fn marks_scale_into_screenshot_pixels() {
        let g = Grounding::Marks(vec![kai_core::Element {
            mark: 1,
            role: kai_core::Role::Button,
            name: "b".into(),
            bounds: Rect::new(10.0, 20.0, 30.0, 40.0),
            enabled: true,
            secure: false,
            value: None,
        }]);
        let Grounding::Marks(m) = scaled(&g, 2.0, (100, 100)) else {
            panic!()
        };
        assert_eq!(m[0].bounds, Rect::new(20.0, 40.0, 60.0, 80.0));
    }
}
