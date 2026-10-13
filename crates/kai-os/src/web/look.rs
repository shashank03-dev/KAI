//! The lane's eyes: the page's tree and layout as marks, the viewport as a
//! screenshot, and an open page dialog as marks of its own (a dialog freezes
//! the page, so nothing else can be read until it is answered).

use super::cdp::DialogOpen;
use super::dialog;
use super::lane::{BrowserLane, MarkTarget, Marked};
use super::snapshot::{self, PageInfo};
use crate::{a11y, frame, marks, zoom};
use async_trait::async_trait;
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
        let (nodes, info) = snapshot::nodes(&ax, &snap, view);
        if !tree_url.is_empty() && !info.url.is_empty() && tree_url != info.url {
            anyhow::bail!("the page changed while it was read");
        }
        let raw: Vec<_> = nodes.iter().map(|(n, _)| n.clone()).collect();
        let (grounding, picked) = a11y::ground_indexed(&raw, view);
        let targets = picked
            .iter()
            .map(|&i| MarkTarget::Dom(nodes[i].1))
            .collect();
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

#[async_trait]
impl Perceptor for BrowserLane {
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

#[cfg(test)]
mod tests {
    use super::*;

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
