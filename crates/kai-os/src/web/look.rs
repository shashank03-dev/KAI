//! The lane's eyes: the page's tree and layout as marks, the viewport as a
//! screenshot, and an open page dialog as marks of its own (a dialog freezes
//! the page, so nothing else can be read until it is answered).

use super::cdp::DialogOpen;
use super::lane::{BrowserLane, MarkTarget, Marked};
use super::snapshot::{self, PageInfo};
use super::{dialog, dropdown};
use crate::{a11y, frame, marks, zoom};
use async_trait::async_trait;
use kai_core::ports::MediaPlayback;
use kai_core::{Grounding, Images, Observation, ObserveTiming, Perceptor, Rect};
use serde_json::json;
use std::time::{Duration, Instant};

/// A read the lane could not finish because a dialog opened.
fn is_dialog(e: &anyhow::Error) -> bool {
    e.downcast_ref::<DialogOpen>().is_some()
}

impl BrowserLane {
    /// The visible area in CSS pixels.
    pub(super) async fn viewport(&self) -> anyhow::Result<Rect> {
        let m = self.page("Page.getLayoutMetrics", json!({})).await?;
        let v = &m["cssVisualViewport"];
        let f = |k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
        Ok(Rect::new(0.0, 0.0, f("clientWidth"), f("clientHeight")))
    }

    /// Tree + snapshot + viewport in parallel, joined into marks. A page that
    /// navigates between the two reads (the tree and the snapshot then
    /// describe different documents) or is mid-load is read once more.
    async fn read_once(&self) -> anyhow::Result<(Grounding, Vec<MarkTarget>, PageInfo, Rect)> {
        let (ax, snap, view) = tokio::join!(
            self.page("Accessibility.getFullAXTree", json!({})),
            self.page(
                "DOMSnapshot.captureSnapshot",
                json!({"computedStyles": ["visibility"]})
            ),
            self.viewport(),
        );
        let (ax, snap, view) = (ax?, snap?, view?);
        let tree_url = ax["nodes"][0]["properties"]
            .as_array()
            .and_then(|p| p.iter().find(|p| p["name"] == "url"))
            .and_then(|p| p["value"]["value"].as_str())
            .unwrap_or("")
            .to_string();
        // Each same-process iframe's tree is its own read; a frame that went
        // away in between is left out.
        let mut trees = vec![ax];
        for frame in snapshot::frames(&snap) {
            match self
                .page("Accessibility.getFullAXTree", json!({"frameId": frame}))
                .await
            {
                Ok(t) => trees.push(t),
                Err(e) if is_dialog(&e) => return Err(e),
                Err(e) => log::debug!("browser lane: frame {frame} unreadable ({e})"),
            }
        }
        let (nodes, info) = snapshot::nodes(&snapshot::merged(trees), &snap, view);
        if !tree_url.is_empty() && !info.url.is_empty() && tree_url != info.url {
            anyhow::bail!("the page changed while it was read");
        }
        let raw: Vec<_> = nodes.iter().map(|(n, _)| n.clone()).collect();
        let (mut grounding, picked) = a11y::ground_indexed(&raw, view);
        let mut targets: Vec<MarkTarget> = picked
            .iter()
            .map(|&i| MarkTarget::Dom(nodes[i].1))
            .collect();
        // An open native dropdown: its options, numbered after the page.
        if let Grounding::Marks(els) = &mut grounding
            && let Some((select, under, options)) = self.open_list().await
        {
            let listed = dropdown::option_marks(&options, under, els.len() as u32 + 1);
            targets.extend((0..listed.len()).map(|index| MarkTarget::Option { select, index }));
            els.extend(listed);
        }
        Ok((grounding, targets, info, view))
    }

    /// Read the page (or the open dialog) and remember the marks. `Ok(None)`
    /// for the image means a dialog is open: there is nothing to capture.
    pub(super) async fn read(&self) -> anyhow::Result<(Grounding, PageInfo, Rect, u64, bool)> {
        let t = Instant::now();
        self.sync_tabs().await?;
        let read = match self.dialog() {
            Some(_) => Err(DialogOpen.into()),
            None => match self.read_once().await {
                Err(e) if !is_dialog(&e) && self.cdp.alive() => {
                    log::debug!("browser lane: reading again ({e})");
                    self.wait_loaded(Duration::from_secs(5)).await;
                    self.read_once().await
                }
                r => r,
            },
        };
        let (grounding, targets, info, view, frozen) = match read {
            Ok((g, t, info, view)) => {
                *self.last.lock().unwrap_or_else(|e| e.into_inner()) = (info.clone(), view);
                (g, t, info, view, false)
            }
            Err(e) if is_dialog(&e) => {
                let (g, t, info, view) = self.dialog_marks();
                (g, t, info, view, true)
            }
            Err(e) => return Err(e),
        };
        *self.marked.lock().unwrap_or_else(|e| e.into_inner()) = Marked {
            grounding: Some(grounding.clone()),
            targets,
        };
        Ok((
            grounding,
            info,
            view,
            t.elapsed().as_millis() as u64,
            frozen,
        ))
    }

    /// The open dialog as marks, with the last known page facts.
    fn dialog_marks(&self) -> (Grounding, Vec<MarkTarget>, PageInfo, Rect) {
        let (info, view) = self.last.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let params = self.dialog().unwrap_or_default();
        let typed = self
            .answer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let (els, targets): (Vec<_>, Vec<_>) = dialog::marks(&params, typed.as_deref(), view)
            .into_iter()
            .map(|(e, t)| (e, MarkTarget::Dialog(t)))
            .unzip();
        *self.last_dialog.lock().unwrap_or_else(|e| e.into_inner()) = els.clone();
        (Grounding::Marks(els), targets, info, view)
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

    /// No image (tree only, or a dialog froze the page).
    fn text_only(grounding: Grounding, info: PageInfo, view: Rect, tree_ms: u64) -> Observation {
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
        Self::observation(grounding, info, view, images, hash, timing)
    }
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

/// Only the focused top-level YouTube watch player can satisfy stop-on-play;
/// embedded videos, audio elements, background tabs and ads are not evidence.
const MEDIA_PLAYBACK_EXPRESSION: &str = r#"(() => {
    if (window.top !== window || document.visibilityState !== 'visible' ||
        !document.hasFocus() ||
        !['http:', 'https:'].includes(location.protocol) ||
        !['youtube.com', 'www.youtube.com', 'm.youtube.com'].includes(location.hostname) ||
        location.pathname !== '/watch') return null;

    const match = /(?:^\?|&)v=([A-Za-z0-9_-]{11})(?:&|$)/.exec(location.search);
    if (!match || (location.search.match(/(?:^\?|&)v=/g) || []).length !== 1) return null;

    const player = document.querySelector('#movie_player.html5-video-player');
    const video = player?.querySelector('video.html5-main-video');
    if (!(video instanceof HTMLVideoElement) || !video.isConnected ||
        !video.currentSrc || video.videoWidth <= 0 || video.videoHeight <= 0 ||
        video.readyState < 2 || !Number.isFinite(video.currentTime) ||
        video.currentTime < 0) return null;

    // During SPA navigation the URL and document title can advance before the
    // video has switched. Bind evidence to the player's actual loaded ID.
    const data = typeof player.getVideoData === 'function' && player.getVideoData();
    if (!data || data.video_id !== match[1]) return null;

    const advertisement = player.classList.contains('ad-showing') ||
        player.classList.contains('ad-interrupting');
    return {
        title: document.title,
        url: location.href,
        current_time: video.currentTime,
        playing: !advertisement && !video.paused && !video.ended && !video.seeking &&
            !video.muted && video.volume > 0 &&
            !(typeof player.isMuted === 'function' && player.isMuted()) &&
            !(typeof player.getVolume === 'function' && player.getVolume() <= 0),
        advertisement
    };
})()"#;

/// Reject a closed/replaced tab or navigation racing the media read. A child
/// tab is not evidence that the opener lost focus: the page probe checks focus.
fn playback_target_current(targets: &serde_json::Value, target: &str, url: &str) -> bool {
    let Some(infos) = targets["targetInfos"].as_array() else {
        return false;
    };
    infos
        .iter()
        .any(|t| t["targetId"] == target && t["type"] == "page" && t["url"] == url)
}

#[async_trait]
impl Perceptor for BrowserLane {
    /// Inspect HTML media state without pressing controls or mutating the page.
    /// The engine samples it twice and requires the playback clock to advance.
    async fn media_playback(&self) -> Option<MediaPlayback> {
        let tab = self.current();
        let r = self
            .page(
                "Runtime.evaluate",
                json!({
                    "expression": MEDIA_PLAYBACK_EXPRESSION,
                    "returnByValue": true,
                }),
            )
            .await
            .ok()?;
        let v = &r["result"]["value"];
        let media = MediaPlayback {
            title: v["title"].as_str()?.to_owned(),
            url: v["url"].as_str()?.to_owned(),
            current_time: v["current_time"].as_f64()?,
            playing: v["playing"].as_bool()?,
            advertisement: v["advertisement"].as_bool()?,
        };
        // Checking target metadata is read-only; sync_tabs() could activate
        // or reload a tab, which a completion probe must never do.
        let targets = self
            .cdp
            .call("Target.getTargets", json!({}), None)
            .await
            .ok()?;
        if self.current().session != tab.session
            || !playback_target_current(&targets, &tab.target, &media.url)
        {
            return None;
        }
        Some(media)
    }

    async fn observe(&self) -> anyhow::Result<Observation> {
        let t = Instant::now();
        let (grounding, info, view, tree_ms, frozen) = self.read().await?;
        if frozen {
            return Ok(Self::text_only(grounding, info, view, tree_ms));
        }
        let img = match self.shot().await {
            Ok(img) => img,
            // A dialog opened between the read and the capture.
            Err(e) if is_dialog(&e) => {
                let (g, info, view, ms, _) = self.read().await?;
                return Ok(Self::text_only(g, info, view, ms));
            }
            Err(e) => return Err(e),
        };
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
        let (grounding, info, view, tree_ms, frozen) = self.read().await?;
        if !frozen && matches!(grounding, Grounding::Grid { .. }) {
            return self.observe().await;
        }
        Ok(Self::text_only(grounding, info, view, tree_ms))
    }

    fn reads_pages(&self) -> bool {
        true
    }

    /// The page's visible text (`innerText`: what a person can read, no
    /// hidden elements, no field values), one tidy line per line of text.
    async fn page_text(&self, max_chars: usize) -> Option<(String, usize)> {
        let r = self
            .page(
                "Runtime.evaluate",
                json!({
                    "expression": "document.body ? document.body.innerText : ''",
                    "returnByValue": true,
                }),
            )
            .await
            .ok()?;
        let raw = r["result"]["value"].as_str()?;
        Some(tidy(raw, max_chars))
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

    /// Settled when the document has loaded and two tree reads agree, or a
    /// dialog is waiting for an answer.
    async fn settle(&self, max: Duration) {
        let deadline = Instant::now() + max;
        tokio::time::sleep(Duration::from_millis(60)).await;
        self.wait_loaded(max).await;
        let mut last = None;
        while Instant::now() < deadline {
            let now = match self.read().await {
                Ok((_, _, _, _, true)) => return,
                Ok((g, ..)) => Some(crate::perceive::tree_hash(&g)),
                Err(_) => None,
            };
            if now.is_some() && now == last {
                return;
            }
            last = now;
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
    }
}

/// One line per line of text, blank runs dropped, spaces collapsed; cut to
/// `max` chars. Returns the text and its full length.
fn tidy(raw: &str, max: usize) -> (String, usize) {
    let text = raw
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let total = text.chars().count();
    (text.chars().take(max).collect(), total)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exercise the exact expression sent to CDP against a minimal DOM fixture.
    fn evaluated_media(overrides: &str) -> serde_json::Value {
        let runtime = rquickjs::Runtime::new().unwrap();
        let context = rquickjs::Context::full(&runtime).unwrap();
        context.with(|ctx| {
            ctx.eval::<(), _>(
                r#"
                globalThis.window = {};
                window.top = window;
                globalThis.documentHasFocus = true;
                globalThis.location = {
                    protocol: 'https:', hostname: 'www.youtube.com', pathname: '/watch',
                    search: '?v=abcdefghijk', href: 'https://www.youtube.com/watch?v=abcdefghijk'
                };
                globalThis.HTMLVideoElement = class {};
                globalThis.video = new HTMLVideoElement();
                Object.assign(video, {
                    isConnected: true, currentSrc: 'blob:video', videoWidth: 1280,
                    videoHeight: 720, readyState: 4, currentTime: 12,
                    paused: false, ended: false, seeking: false,
                    muted: false, volume: 1
                });
                globalThis.player = {
                    ad: '',
                    videoId: 'abcdefghijk',
                    getVideoData() { return { video_id: this.videoId }; },
                    muted: false,
                    volume: 100,
                    isMuted() { return this.muted; },
                    getVolume() { return this.volume; },
                    classList: { contains(name) { return player.ad === name; } },
                    querySelector(selector) {
                        return selector === 'video.html5-main-video' ? video : null;
                    }
                };
                globalThis.document = {
                    title: 'Song - YouTube', visibilityState: 'visible',
                    hasFocus() { return documentHasFocus; },
                    querySelector(selector) {
                        if (selector === '#movie_player.html5-video-player') return player;
                        if (selector === 'video.html5-main-video, video, audio') return video;
                        return null;
                    }
                };
                "#,
            )
            .unwrap();
            ctx.eval::<(), _>(overrides).unwrap();
            let json: String = ctx
                .eval(format!("JSON.stringify({MEDIA_PLAYBACK_EXPRESSION})"))
                .unwrap();
            serde_json::from_str(&json).unwrap()
        })
    }

    #[test]
    fn media_inspection_requires_visible_top_level_youtube_watch_player() {
        assert_eq!(evaluated_media("")["playing"], true);
        for change in [
            "window.top = {};",
            "document.visibilityState = 'hidden';",
            "documentHasFocus = false;",
            "location.hostname = 'music.youtube.com';",
            "location.protocol = 'file:';",
            "location.pathname = '/results';",
            "location.search = '?q=abcdefghijk';",
            "location.search = '?v=too-short';",
            "location.search = '?v=lmnopqrstuv&v=abcdefghijk';",
            "player.querySelector = () => null;",
            "video.isConnected = false;",
            "video.videoWidth = 0;",
            "video.currentSrc = '';",
            "video.readyState = 1;",
            "video.currentTime = NaN;",
            "player.getVideoData = undefined;",
            "player.videoId = 'differentid';",
            "location.search = '?v=differentid'; document.title = 'Other song - YouTube';",
        ] {
            assert!(evaluated_media(change).is_null(), "{change}");
        }
    }

    #[test]
    fn media_inspection_rejects_muted_and_ad_playback() {
        for change in [
            "video.muted = true;",
            "video.volume = 0;",
            "player.muted = true;",
            "player.volume = 0;",
            "video.paused = true;",
            "video.ended = true;",
            "video.seeking = true;",
        ] {
            assert_eq!(evaluated_media(change)["playing"], false, "{change}");
        }
        for ad in ["ad-showing", "ad-interrupting"] {
            let media = evaluated_media(&format!("player.ad = '{ad}';"));
            assert_eq!(media["advertisement"], true);
            assert_eq!(media["playing"], false);
        }
    }

    #[test]
    fn media_inspection_requires_loaded_video_id_after_spa_navigation() {
        assert_eq!(evaluated_media("")["playing"], true);
        assert!(
            evaluated_media(
                "location.search = '?v=lmnopqrstuv'; document.title = 'Next song - YouTube';"
            )
            .is_null()
        );
        let switched =
            evaluated_media("location.search = '?v=lmnopqrstuv'; player.videoId = 'lmnopqrstuv';");
        assert_eq!(switched["playing"], true);
        assert!(evaluated_media("player.videoId = ''; ").is_null());
        assert_eq!(
            evaluated_media("location.search = '?t=20&v=abcdefghijk&list=playlist';")["playing"],
            true
        );
    }

    #[test]
    fn media_inspection_rejects_stale_target_or_pending_child_tab() {
        let watch = "https://www.youtube.com/watch?v=abcdefghijk";
        let current = json!({"targetInfos": [
            {"targetId": "active", "type": "page", "url": watch}
        ]});
        assert!(playback_target_current(&current, "active", watch));
        assert!(!playback_target_current(&current, "closed", watch));
        assert!(!playback_target_current(
            &current,
            "active",
            "https://www.youtube.com/"
        ));
        let switched = json!({"targetInfos": [
            {"targetId": "active", "type": "page", "url": watch},
            {"targetId": "child", "type": "page", "openerId": "active", "url": "about:blank"}
        ]});
        assert!(playback_target_current(&switched, "active", watch));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn media_inspection_real_cdp_is_read_only_and_tracks_target() {
        use super::super::lane::{LaneOptions, find_browser};

        let Some(browser) = find_browser() else {
            assert!(
                std::env::var_os("KAI_REQUIRE_BROWSER").is_none(),
                "KAI_REQUIRE_BROWSER requires Chromium for this test"
            );
            eprintln!("no Chromium-family browser found; skipping real CDP probe");
            return;
        };
        let profile = tempfile::tempdir().unwrap();
        let lane = BrowserLane::launch(
            &LaneOptions {
                browser,
                profile: profile.path().to_path_buf(),
                headless: true,
                size: (800, 600),
                downloads: None,
            },
            None,
        )
        .await
        .unwrap();

        let tab = lane.current();
        lane.cdp
            .call(
                "Target.activateTarget",
                json!({"targetId": tab.target}),
                None,
            )
            .await
            .unwrap();
        lane.page(
            "Runtime.evaluate",
            json!({"expression": "document.title = 'Song - YouTube'; window.probeSentinel = 17"}),
        )
        .await
        .unwrap();
        assert!(
            lane.media_playback().await.is_none(),
            "about:blank is not YouTube"
        );
        let inspect = |expression| json!({"expression": expression, "returnByValue": true});
        let before = lane
            .page(
                "Runtime.evaluate",
                inspect("[document.title, window.probeSentinel, document.hasFocus()]"),
            )
            .await
            .unwrap();
        assert_eq!(before["result"]["value"][0], "Song - YouTube");
        assert_eq!(before["result"]["value"][1], 17);
        assert_eq!(before["result"]["value"][2], true);

        let child = lane
            .cdp
            .call("Target.createTarget", json!({"url": "about:blank"}), None)
            .await
            .unwrap();
        lane.cdp
            .call(
                "Target.activateTarget",
                json!({"targetId": child["targetId"]}),
                None,
            )
            .await
            .unwrap();
        let focus = lane
            .page(
                "Runtime.evaluate",
                inspect("[document.hasFocus(), document.visibilityState]"),
            )
            .await
            .unwrap();
        let targets = lane
            .cdp
            .call("Target.getTargets", json!({}), None)
            .await
            .unwrap();
        assert_eq!(lane.current().session, tab.session);
        assert!(playback_target_current(
            &targets,
            &tab.target,
            "about:blank"
        ));
        // Headless Chromium can report `hasFocus() == true` and `visible` for
        // both pages. This signal is best-effort, not a browser-tab mute or
        // definitive foreground oracle in headless mode.
        assert!(focus["result"]["value"][0].as_bool().is_some());
        assert!(focus["result"]["value"][1].as_str().is_some());
    }

    #[test]
    fn page_text_is_tidied_and_cut() {
        let (t, n) = tidy("  Prices \n\n\n\tBlue   Kettle\t34.99\n   \n", 100);
        assert_eq!(t, "Prices\nBlue Kettle 34.99");
        assert_eq!(n, t.chars().count());
        let (cut, total) = tidy("abcdef", 3);
        assert_eq!((cut.as_str(), total), ("abc", 6));
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
            focused: false,
            value: None,
        }]);
        let Grounding::Marks(m) = scaled(&g, 2.0, (100, 100)) else {
            panic!()
        };
        assert_eq!(m[0].bounds, Rect::new(20.0, 40.0, 60.0, 80.0));
    }
}
