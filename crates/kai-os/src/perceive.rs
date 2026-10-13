//! The real eyes: [`DesktopPerceptor`] captures the primary display, reads
//! the accessibility tree and grounds it; it can also observe the tree alone
//! (no pixels) and zoom into a region for a second look.

use crate::{a11y, frame, marks, zoom};
use async_trait::async_trait;
use image::RgbaImage;
use kai_core::{Grounding, Images, Observation, ObserveTiming, Perceptor, Rect};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The primary display and its place in global coordinates.
fn primary() -> anyhow::Result<xcap::Monitor> {
    let all = xcap::Monitor::all()?;
    let primary = all
        .iter()
        .position(|m| m.is_primary().unwrap_or(false))
        .unwrap_or(0);
    all.into_iter()
        .nth(primary)
        .ok_or_else(|| anyhow::anyhow!("no display found"))
}

/// The primary display's rect without capturing it.
fn primary_rect() -> anyhow::Result<Rect> {
    let m = primary()?;
    Ok(Rect::new(
        m.x()? as f32,
        m.y()? as f32,
        m.width()? as f32,
        m.height()? as f32,
    ))
}

fn grab() -> anyhow::Result<(RgbaImage, Rect)> {
    let m = primary()?;
    let img = m.capture_image()?;
    let at = Rect::new(
        m.x()? as f32,
        m.y()? as f32,
        img.width() as f32,
        img.height() as f32,
    );
    Ok((img, at))
}

/// Captures the primary display, reads the accessibility tree and grounds it.
#[derive(Default)]
pub struct DesktopPerceptor {
    /// Kai's own windows, painted over where the OS cannot exclude them.
    excluded: Mutex<Vec<Rect>>,
}

impl DesktopPerceptor {
    pub fn new() -> Self {
        Self::default()
    }

    /// The app reports its visible windows here (global screen pixels).
    pub fn set_excluded(&self, rects: Vec<Rect>) {
        *self.excluded.lock().expect("not poisoned") = rects;
    }

    fn excluded(&self) -> Vec<Rect> {
        self.excluded.lock().map(|e| e.clone()).unwrap_or_default()
    }

    async fn hash_now(&self) -> Option<u64> {
        let excluded = self.excluded();
        tokio::task::spawn_blocking(move || {
            let (mut img, at) = grab().ok()?;
            frame::mask(&mut img, &local(&excluded, at));
            Some(frame::dhash(&frame::downscale(&img).image))
        })
        .await
        .ok()?
    }
}

pub(crate) fn local(rects: &[Rect], at: Rect) -> Vec<Rect> {
    rects
        .iter()
        .map(|r| Rect::new(r.x - at.x, r.y - at.y, r.w, r.h))
        .collect()
}

/// Shift a grounding from global coordinates into the captured image's.
pub(crate) fn shifted(g: &Grounding, at: Rect) -> Grounding {
    match g {
        Grounding::Marks(els) => Grounding::Marks(
            els.iter()
                .map(|e| {
                    let mut e = e.clone();
                    e.bounds =
                        Rect::new(e.bounds.x - at.x, e.bounds.y - at.y, e.bounds.w, e.bounds.h);
                    e
                })
                .collect(),
        ),
        Grounding::Grid { cols, rows, .. } => Grounding::Grid {
            cols: *cols,
            rows: *rows,
            screen: Rect::new(0.0, 0.0, at.w, at.h),
        },
    }
}

/// A stable hash of what the tree shows (FNV-1a over the element list and
/// boxes), so "nothing changed" detection works without pixels. It cannot
/// see changes the tree does not report (text typed into a field, a checked
/// box), which is why the engine should only use tree-only steps sparingly.
pub(crate) fn tree_hash(g: &Grounding) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    eat(g.as_text().as_bytes());
    if let Grounding::Marks(els) = g {
        for e in els {
            for v in [e.bounds.x, e.bounds.y, e.bounds.w, e.bounds.h] {
                eat(&(v.round() as i32).to_le_bytes());
            }
        }
    }
    h
}

impl DesktopPerceptor {
    /// Capture + tree + ground + draw + encode. `tree` reuses a tree that
    /// was just read instead of reading it again.
    async fn full(
        &self,
        tree: Option<(Vec<a11y::RawNode>, Duration)>,
    ) -> anyhow::Result<Observation> {
        let excluded = self.excluded();

        // Capture and the tree read run in parallel, both off the UI thread.
        let capture = tokio::task::spawn_blocking(move || {
            let t = Instant::now();
            grab().map(|g| (g, t.elapsed()))
        });
        let tree = async {
            match tree {
                Some(read) => read,
                None => {
                    let t = Instant::now();
                    (a11y::read_tree().await, t.elapsed())
                }
            }
        };
        let (shot, (nodes, tree_time)) = tokio::join!(capture, tree);
        let ((mut img, at), capture_time) = shot??;

        let t_enc = Instant::now();
        let (images, grounding, hash) = tokio::task::spawn_blocking(move || {
            frame::mask(&mut img, &local(&excluded, at));
            let grounding = a11y::ground_live(&nodes, at);
            let f = frame::downscale(&img);
            let hash = frame::content_hash(&f.image);
            let mut marked = f.image;
            marks::draw(&mut marked, &shifted(&grounding, at), f.scale);
            let images = Images {
                screenshot_jpeg: frame::encode_jpeg(&marked),
                crop_jpeg: None,
            };
            (images, grounding, hash)
        })
        .await?;

        Ok(Observation {
            images,
            grounding,
            screen_hash: hash,
            timing: ObserveTiming {
                capture_ms: capture_time.as_millis() as u64,
                tree_ms: tree_time.as_millis() as u64,
                encode_ms: t_enc.elapsed().as_millis() as u64,
            },
            screen: Some(at),
            window_title: a11y::active::title(),
            url: None,
        })
    }
}

#[async_trait]
impl Perceptor for DesktopPerceptor {
    async fn observe(&self) -> anyhow::Result<Observation> {
        self.full(None).await
    }

    /// The tree alone: no capture, no encode, an empty screenshot. Falls back
    /// to a full observation when the tree is not useful (grid fallback),
    /// since a grid without pixels tells the model nothing.
    async fn observe_tree_only(&self) -> anyhow::Result<Observation> {
        let t = Instant::now();
        let (nodes, screen) =
            tokio::join!(a11y::read_tree(), tokio::task::spawn_blocking(primary_rect));
        let tree_time = t.elapsed();
        let screen = screen??;
        let grounding = a11y::ground_live(&nodes, screen);
        if matches!(grounding, Grounding::Grid { .. }) {
            return self.full(Some((nodes, tree_time))).await;
        }
        Ok(Observation {
            images: Images {
                screenshot_jpeg: Vec::new(),
                crop_jpeg: None,
            },
            screen_hash: tree_hash(&grounding),
            grounding,
            timing: ObserveTiming {
                capture_ms: 0,
                tree_ms: tree_time.as_millis() as u64,
                encode_ms: 0,
            },
            screen: Some(screen),
            window_title: a11y::active::title(),
            url: None,
        })
    }

    /// Capture, crop the padded region, upscale small regions ~2x and draw a
    /// numbered sub-grid. Kai's own windows stay masked.
    async fn zoom(&self, rect: Rect) -> Option<Vec<u8>> {
        let excluded = self.excluded();
        tokio::task::spawn_blocking(move || {
            let (mut img, at) = grab().ok()?;
            let plan = zoom::plan(rect, at)?;
            frame::mask(&mut img, &local(&excluded, at));
            let out = zoom::render(&img, at, &plan);
            Some(frame::encode_jpeg(&out))
        })
        .await
        .ok()?
    }

    /// AT-SPI events from the active app (Linux); `None` on Windows.
    fn ui_events(&self) -> Option<u64> {
        a11y::ui_events()
    }

    /// Wait until two captures in a row match, or `max` passes.
    async fn settle(&self, max: Duration) {
        let deadline = Instant::now() + max;
        tokio::time::sleep(Duration::from_millis(120)).await;
        let mut last = self.hash_now().await;
        while Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let now = self.hash_now().await;
            match (last, now) {
                (Some(a), Some(b)) if frame::same_screen(a, b) => return,
                (_, None) => return,
                _ => last = now,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kai_core::{Element, Role};

    fn one(x: f32, name: &str) -> Grounding {
        Grounding::Marks(vec![Element {
            mark: 1,
            role: Role::Button,
            name: name.into(),
            bounds: Rect::new(x, 30.0, 10.0, 10.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }])
    }

    #[test]
    fn grounding_shifts_into_image_space() {
        let Grounding::Marks(m) =
            shifted(&one(2020.0, "x"), Rect::new(1920.0, 0.0, 1920.0, 1080.0))
        else {
            panic!()
        };
        assert_eq!(m[0].bounds.x, 100.0);
    }

    #[test]
    fn tree_hash_tracks_names_and_boxes() {
        assert_eq!(tree_hash(&one(10.0, "a")), tree_hash(&one(10.0, "a")));
        assert_ne!(tree_hash(&one(10.0, "a")), tree_hash(&one(10.0, "b")));
        assert_ne!(tree_hash(&one(10.0, "a")), tree_hash(&one(50.0, "a")));
    }
}
