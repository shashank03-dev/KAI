//! The real eyes and hands: [`DesktopPerceptor`] and [`DesktopActuator`]
//! implement kai-core's traits for the machine Kai runs on.

use crate::{a11y, frame, input::Input, kill::KillSwitch, launch, marks, shell::Shell};
use async_trait::async_trait;
use image::RgbaImage;
use kai_core::{
    Action, Actuator, Decision, Grounding, Images, Observation, ObserveTiming, Perceptor, Rect,
};
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

    async fn hash_now(&self) -> Option<u64> {
        let excluded = self.excluded.lock().ok()?.clone();
        tokio::task::spawn_blocking(move || {
            let (mut img, at) = grab().ok()?;
            frame::mask(&mut img, &local(&excluded, at));
            Some(frame::dhash(&frame::downscale(&img).image))
        })
        .await
        .ok()?
    }
}

fn local(rects: &[Rect], at: Rect) -> Vec<Rect> {
    rects
        .iter()
        .map(|r| Rect::new(r.x - at.x, r.y - at.y, r.w, r.h))
        .collect()
}

/// Shift a grounding from global coordinates into the captured image's.
fn shifted(g: &Grounding, at: Rect) -> Grounding {
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

#[async_trait]
impl Perceptor for DesktopPerceptor {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let excluded = self.excluded.lock().expect("not poisoned").clone();

        // Capture and the tree read run in parallel, both off the UI thread.
        let capture = tokio::task::spawn_blocking(move || {
            let t = Instant::now();
            grab().map(|g| (g, t.elapsed()))
        });
        let tree = async {
            let t = Instant::now();
            (a11y::read_tree().await, t.elapsed())
        };
        let (shot, (nodes, tree_time)) = tokio::join!(capture, tree);
        let ((mut img, at), capture_time) = shot??;

        let t_enc = Instant::now();
        let (images, grounding, hash) = tokio::task::spawn_blocking(move || {
            frame::mask(&mut img, &local(&excluded, at));
            let grounding = a11y::ground(&nodes, at);
            let f = frame::downscale(&img);
            let hash = frame::dhash(&f.image);
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
        })
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

/// Mouse, keyboard, launcher and terminal.
pub struct DesktopActuator {
    pub input: Input,
    pub shell: Shell,
}

impl DesktopActuator {
    pub fn new(kill: KillSwitch) -> anyhow::Result<Self> {
        Ok(Self {
            input: Input::spawn()?,
            shell: Shell::new(kill),
        })
    }

    fn point(decision: &Decision, grounding: &Grounding) -> anyhow::Result<(i32, i32)> {
        let target = decision
            .target
            .ok_or_else(|| anyhow::anyhow!("no target"))?;
        let r = grounding
            .target_rect(target)
            .ok_or_else(|| anyhow::anyhow!("target #{target} is not on screen"))?;
        let (x, y) = r.center();
        Ok((x.round() as i32, y.round() as i32))
    }
}

#[async_trait]
impl Actuator for DesktopActuator {
    async fn act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        match d.action {
            Action::Click => {
                let (x, y) = Self::point(d, g)?;
                self.input.click(x, y, false, 1).await
            }
            Action::DoubleClick => {
                let (x, y) = Self::point(d, g)?;
                self.input.click(x, y, false, 2).await
            }
            Action::RightClick => {
                let (x, y) = Self::point(d, g)?;
                self.input.click(x, y, true, 1).await
            }
            Action::Type => {
                if let Ok((x, y)) = Self::point(d, g) {
                    self.input.click(x, y, false, 1).await?;
                    tokio::time::sleep(Duration::from_millis(60)).await;
                }
                self.input.type_text(d.text.as_deref().unwrap_or("")).await
            }
            Action::Key => self.input.keys(&d.keys).await,
            Action::Scroll => {
                let (x, y) = match Self::point(d, g) {
                    Ok(p) => p,
                    Err(_) => self.input.cursor().await?,
                };
                self.input
                    .scroll(x, y, d.direction.unwrap_or_default(), d.amount.unwrap_or(3))
                    .await
            }
            Action::OpenApp => self.open_app(d.text.as_deref().unwrap_or("")).await,
            Action::OpenUrl => self.open_url(d.text.as_deref().unwrap_or("")).await,
            Action::Shell => self.shell(d.text.as_deref().unwrap_or("")).await.map(drop),
            Action::Wait | Action::AskUser | Action::Done => Ok(()),
        }
    }

    async fn open_app(&self, app: &str) -> anyhow::Result<()> {
        let app = app.to_string();
        tokio::task::spawn_blocking(move || launch::open_app(&app)).await?
    }

    async fn open_url(&self, url: &str) -> anyhow::Result<()> {
        let url = url.to_string();
        tokio::task::spawn_blocking(move || launch::open_url(&url)).await?
    }

    async fn shell(&self, cmd: &str) -> anyhow::Result<String> {
        self.shell.run(cmd).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kai_core::{Element, Role};

    #[test]
    fn grounding_shifts_into_image_space() {
        let g = Grounding::Marks(vec![Element {
            mark: 1,
            role: Role::Button,
            name: "x".into(),
            bounds: Rect::new(2020.0, 30.0, 10.0, 10.0),
            enabled: true,
            secure: false,
        }]);
        let Grounding::Marks(m) = shifted(&g, Rect::new(1920.0, 0.0, 1920.0, 1080.0)) else {
            panic!()
        };
        assert_eq!(m[0].bounds.x, 100.0);
    }

    #[test]
    fn click_point_is_element_centre() {
        let g = Grounding::Marks(vec![Element {
            mark: 4,
            role: Role::Button,
            name: "x".into(),
            bounds: Rect::new(10.0, 20.0, 30.0, 40.0),
            enabled: true,
            secure: false,
        }]);
        let d =
            Decision::parse_checked(r#"{"thought":"","action":"click","target":4,"risk":"none"}"#)
                .unwrap();
        assert_eq!(DesktopActuator::point(&d, &g).unwrap(), (25, 40));
        let missing =
            Decision::parse_checked(r#"{"thought":"","action":"click","target":9,"risk":"none"}"#)
                .unwrap();
        assert!(DesktopActuator::point(&missing, &g).is_err());
    }
}
