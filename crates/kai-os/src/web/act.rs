//! The lane's hands. Every event goes to the page through the protocol, so
//! the user's pointer never moves and no window takes focus: Kai works in
//! its browser while the user works elsewhere.

use super::keys::{self, Chord};
use super::lane::BrowserLane;
use async_trait::async_trait;
use kai_core::{Action, Actuator, Decision, Direction, Grounding};
use serde_json::{Value, json};
use std::time::Duration;

/// Longest a navigation may take before the step fails.
const LOAD_LIMIT: Duration = Duration::from_secs(20);

impl BrowserLane {
    /// The DOM node behind `mark`, if the latest observation numbered this
    /// same element (role, name and box all match); `None` when the marks
    /// are stale, and the caller then uses the box from the grounding.
    fn node_for(&self, mark: u32, g: &Grounding) -> Option<i64> {
        let m = self.marked.lock().ok()?;
        let seen = m.grounding.as_ref()?.element(mark)?;
        let asked = g.element(mark)?;
        (seen == asked)
            .then(|| m.nodes.get(mark as usize - 1).copied())
            .flatten()
    }

    /// Where to point for `d`, in viewport CSS pixels: the DOM node's live
    /// box after scrolling it into view, else the grounding's box (grid
    /// cells, stale marks).
    async fn point(&self, d: &Decision, g: &Grounding) -> anyhow::Result<(f64, f64)> {
        let mark = d.target.ok_or_else(|| anyhow::anyhow!("no target"))?;
        if let Some(id) = self.node_for(mark, g) {
            let _ = self
                .page("DOM.scrollIntoViewIfNeeded", json!({"backendNodeId": id}))
                .await;
            if let Ok(q) = self
                .page("DOM.getContentQuads", json!({"backendNodeId": id}))
                .await
                && let Some(c) = quad_centre(&q["quads"][0])
            {
                return Ok(c);
            }
        }
        let r = g
            .target_rect(mark)
            .ok_or_else(|| anyhow::anyhow!("target #{mark} is not on the page"))?;
        let (x, y) = r.center();
        Ok((x as f64, y as f64))
    }

    async fn mouse(
        &self,
        kind: &str,
        x: f64,
        y: f64,
        button: &str,
        clicks: u32,
    ) -> anyhow::Result<()> {
        self.page(
            "Input.dispatchMouseEvent",
            json!({"type": kind, "x": x, "y": y, "button": button, "clickCount": clicks}),
        )
        .await
        .map(drop)
    }

    async fn click(&self, (x, y): (f64, f64), button: &str, times: u32) -> anyhow::Result<()> {
        self.mouse("mouseMoved", x, y, "none", 0).await?;
        for n in 1..=times {
            self.mouse("mousePressed", x, y, button, n).await?;
            self.mouse("mouseReleased", x, y, button, n).await?;
        }
        Ok(())
    }

    async fn press(&self, chord: &[String]) -> anyhow::Result<()> {
        match keys::chord(chord)? {
            Chord::Keys(downs) => {
                for down in downs {
                    self.page("Input.dispatchKeyEvent", down.clone()).await?;
                    self.page("Input.dispatchKeyEvent", keys::key_up(&down))
                        .await?;
                }
                Ok(())
            }
            Chord::Back => self.history(-1).await,
            Chord::Forward => self.history(1).await,
            Chord::Reload => {
                self.page("Page.reload", json!({})).await?;
                self.wait_loaded(LOAD_LIMIT).await;
                Ok(())
            }
        }
    }

    async fn history(&self, delta: i64) -> anyhow::Result<()> {
        let h = self.page("Page.getNavigationHistory", json!({})).await?;
        let at = h["currentIndex"].as_i64().unwrap_or(0) + delta;
        let entry = usize::try_from(at)
            .ok()
            .and_then(|i| h["entries"].get(i))
            .and_then(|e| e["id"].as_i64())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no page to go {}",
                    if delta < 0 { "back to" } else { "forward to" }
                )
            })?;
        self.page("Page.navigateToHistoryEntry", json!({"entryId": entry}))
            .await?;
        self.wait_loaded(LOAD_LIMIT).await;
        Ok(())
    }

    async fn scroll(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        let (x, y) = match self.point(d, g).await {
            Ok(p) => p,
            Err(_) => {
                // No target: the middle of the viewport.
                let (x, y) = self.viewport_centre().await.unwrap_or((1.0, 1.0));
                (x as f64, y as f64)
            }
        };
        let step = 100.0 * d.amount.unwrap_or(3) as f64;
        let (dx, dy) = match d.direction.unwrap_or_default() {
            Direction::Down => (0.0, step),
            Direction::Up => (0.0, -step),
            Direction::Right => (step, 0.0),
            Direction::Left => (-step, 0.0),
        };
        self.page(
            "Input.dispatchMouseEvent",
            json!({"type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy}),
        )
        .await
        .map(drop)
    }

    async fn viewport_centre(&self) -> Option<(f32, f32)> {
        let m = self.page("Page.getLayoutMetrics", json!({})).await.ok()?;
        let v = &m["cssVisualViewport"];
        Some((
            v["clientWidth"].as_f64()? as f32 / 2.0,
            v["clientHeight"].as_f64()? as f32 / 2.0,
        ))
    }

    /// Navigate after the lane's address policy (`kai_core::web`).
    pub async fn navigate(&self, raw: &str) -> anyhow::Result<()> {
        let url = kai_core::web::normalise_url(raw).map_err(anyhow::Error::msg)?;
        let r = self.page("Page.navigate", json!({"url": url})).await?;
        if let Some(e) = r["errorText"].as_str().filter(|e| !e.is_empty()) {
            anyhow::bail!("could not open {url}: {e}");
        }
        self.wait_loaded(LOAD_LIMIT).await;
        Ok(())
    }
}

fn quad_centre(q: &Value) -> Option<(f64, f64)> {
    let q = q.as_array()?;
    if q.len() < 8 {
        return None;
    }
    let v: Vec<f64> = q.iter().filter_map(Value::as_f64).collect();
    if v.len() < 8 {
        return None;
    }
    Some((
        (v[0] + v[2] + v[4] + v[6]) / 4.0,
        (v[1] + v[3] + v[5] + v[7]) / 4.0,
    ))
}

#[async_trait]
impl Actuator for BrowserLane {
    async fn act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        match d.action {
            Action::Click => self.click(self.point(d, g).await?, "left", 1).await,
            Action::DoubleClick => self.click(self.point(d, g).await?, "left", 2).await,
            Action::RightClick => self.click(self.point(d, g).await?, "right", 1).await,
            Action::Type => {
                if d.target.is_some() {
                    self.click(self.point(d, g).await?, "left", 1).await?;
                }
                let text = d.text.as_deref().unwrap_or("");
                self.page("Input.insertText", json!({"text": text}))
                    .await
                    .map(drop)
            }
            Action::Key => self.press(&d.keys).await,
            Action::Scroll => self.scroll(d, g).await,
            Action::OpenUrl => self.navigate(d.text.as_deref().unwrap_or("")).await,
            Action::OpenApp => self.open_app(d.text.as_deref().unwrap_or("")).await,
            Action::Shell => self.shell(d.text.as_deref().unwrap_or("")).await.map(drop),
            Action::Wait | Action::AskUser | Action::Done => Ok(()),
        }
    }

    async fn open_app(&self, app: &str) -> anyhow::Result<()> {
        match &self.fallback {
            Some(desktop) => desktop.open_app(app).await,
            None => anyhow::bail!("opening apps is not available in browser-only mode"),
        }
    }

    async fn open_url(&self, url: &str) -> anyhow::Result<()> {
        self.navigate(url).await
    }

    async fn shell(&self, cmd: &str) -> anyhow::Result<String> {
        match &self.fallback {
            Some(desktop) => desktop.shell(cmd).await,
            None => anyhow::bail!("the terminal is not available in browser-only mode"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_centre_is_the_mean_of_four_corners() {
        let q = json!([10.0, 20.0, 30.0, 20.0, 30.0, 40.0, 10.0, 40.0]);
        assert_eq!(quad_centre(&q), Some((20.0, 30.0)));
        assert_eq!(quad_centre(&json!([1.0, 2.0])), None);
        assert_eq!(quad_centre(&Value::Null), None);
    }
}
