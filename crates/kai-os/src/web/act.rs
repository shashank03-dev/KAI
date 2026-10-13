//! The lane's hands. Every event goes to the page through the protocol, so
//! the user's pointer never moves and no window takes focus: Kai works in
//! its browser while the user works elsewhere.

use super::cdp::DialogOpen;
use super::dialog::{self, DialogTarget};
use super::keys::{self, Chord};
use super::lane::{BrowserLane, MarkTarget};
use async_trait::async_trait;
use kai_core::{Action, Actuator, Decision, Direction, Grounding};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Longest a navigation may take before the step fails.
const LOAD_LIMIT: Duration = Duration::from_secs(20);

/// An input call that a page dialog interrupted did its job: the click or
/// key landed and the page answered with a dialog. The next observation
/// shows the dialog.
fn landed(r: anyhow::Result<()>) -> anyhow::Result<()> {
    match r {
        Err(e) if e.downcast_ref::<DialogOpen>().is_some() => Ok(()),
        r => r,
    }
}

impl BrowserLane {
    /// What `mark` stands for, if the latest observation numbered this same
    /// element (role, name and box match; its value and enabled state may
    /// have changed since, e.g. by Kai's own typing). `None` when the marks
    /// are stale; the caller then uses the box from the grounding.
    fn target_for(&self, mark: u32, g: &Grounding) -> Option<MarkTarget> {
        let m = self.marked.lock().ok()?;
        let seen = m.grounding.as_ref()?.element(mark)?;
        let asked = g.element(mark)?;
        let same =
            seen.role == asked.role && seen.name == asked.name && seen.bounds == asked.bounds;
        same.then(|| m.targets.get(mark.checked_sub(1)? as usize).copied())
            .flatten()
    }

    /// Where to point for `mark`, in viewport CSS pixels: the DOM node's
    /// live box after scrolling it into view, else the grounding's box (grid
    /// cells, stale marks).
    async fn point(&self, mark: u32, g: &Grounding) -> anyhow::Result<(f64, f64)> {
        if let Some(MarkTarget::Dom(r)) = self.target_for(mark, g) {
            let _ = self
                .page("DOM.scrollIntoViewIfNeeded", json!({"backendNodeId": r.id}))
                .await;
            if let Ok(q) = self
                .page("DOM.getContentQuads", json!({"backendNodeId": r.id}))
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
        self.file_chooser.store(false, Ordering::SeqCst);
        self.mouse("mouseMoved", x, y, "none", 0).await?;
        for n in 1..=times {
            self.mouse("mousePressed", x, y, button, n).await?;
            self.mouse("mouseReleased", x, y, button, n).await?;
        }
        // A file picker opens asynchronously; give its event a moment.
        tokio::time::sleep(Duration::from_millis(30)).await;
        self.sync_tabs().await?;
        if self.file_chooser.swap(false, Ordering::SeqCst) {
            anyhow::bail!(
                "that opened a file picker; Kai's browser cannot choose files yet. Ask the \
                 user to attach the file (ask_user), or find another way"
            );
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
        let at = match d.target {
            Some(mark) => self.point(mark, g).await.ok(),
            None => None,
        };
        let (x, y) = match at {
            Some(p) => p,
            None => {
                let v = self.viewport().await?;
                let (x, y) = v.center();
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

    /// Answer the open page dialog.
    async fn answer(&self, accept: bool) -> anyhow::Result<()> {
        let session = self.current().session;
        let mut params = json!({ "accept": accept });
        if accept && let Some(text) = self.answer.lock().unwrap_or_else(|e| e.into_inner()).take() {
            params["promptText"] = json!(text);
        }
        self.cdp
            .call("Page.handleJavaScriptDialog", params, Some(&session))
            .await?;
        self.cdp.clear_dialog(&session);
        // A `beforeunload` answer lets the navigation go on.
        self.wait_loaded(LOAD_LIMIT).await;
        Ok(())
    }

    /// While a dialog is open only its own marks, Enter and Esc work.
    async fn act_on_dialog(&self, d: &Decision, g: &Grounding, open: &Value) -> anyhow::Result<()> {
        let target = d.target.and_then(|m| self.target_for(m, g));
        match (d.action, target) {
            (
                Action::Click | Action::DoubleClick,
                Some(MarkTarget::Dialog(DialogTarget::Accept)),
            ) => self.answer(true).await,
            (
                Action::Click | Action::DoubleClick,
                Some(MarkTarget::Dialog(DialogTarget::Dismiss)),
            ) => self.answer(false).await,
            (Action::Type, Some(MarkTarget::Dialog(DialogTarget::Field))) => {
                *self.answer.lock().unwrap_or_else(|e| e.into_inner()) = d.text.clone();
                Ok(())
            }
            (Action::Click, Some(MarkTarget::Dialog(DialogTarget::Field))) => Ok(()),
            (Action::Key, _) if d.keys == ["enter"] => self.answer(true).await,
            (Action::Key, _) if d.keys == ["esc"] => self.answer(false).await,
            (Action::Wait | Action::AskUser | Action::Done, _) => Ok(()),
            _ => anyhow::bail!(
                "{}: answer it first (click OK or Cancel)",
                dialog::describe(open)
            ),
        }
    }

    /// Navigate after the lane's address policy (`kai_core::web`).
    pub async fn navigate(&self, raw: &str) -> anyhow::Result<()> {
        let url = kai_core::web::normalise_url(raw).map_err(anyhow::Error::msg)?;
        if let Some(open) = self.dialog() {
            anyhow::bail!("{}: answer it first", dialog::describe(&open));
        }
        let r = match self.page("Page.navigate", json!({"url": url})).await {
            Ok(r) => r,
            // A `beforeunload` dialog asks first; the model answers it next.
            Err(e) if e.downcast_ref::<DialogOpen>().is_some() => return Ok(()),
            Err(e) => return Err(e),
        };
        if let Some(e) = r["errorText"].as_str().filter(|e| !e.is_empty()) {
            anyhow::bail!("could not open {url}: {e}");
        }
        self.wait_loaded(LOAD_LIMIT).await;
        Ok(())
    }
}

/// The option's own text from its mark name (`Billing (option)`).
fn option_label(name: &str) -> String {
    name.trim_end_matches(" (selected option)")
        .trim_end_matches(" (option)")
        .to_string()
}

fn quad_centre(q: &Value) -> Option<(f64, f64)> {
    let v: Vec<f64> = q.as_array()?.iter().filter_map(Value::as_f64).collect();
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
        if matches!(d.action, Action::OpenApp | Action::Shell | Action::OpenUrl) {
            return match d.action {
                Action::OpenApp => self.open_app(d.text.as_deref().unwrap_or("")).await,
                Action::Shell => self.shell(d.text.as_deref().unwrap_or("")).await.map(drop),
                _ => self.navigate(d.text.as_deref().unwrap_or("")).await,
            };
        }
        self.sync_tabs().await?;
        if let Some(open) = self.dialog() {
            return self.act_on_dialog(d, g, &open).await;
        }
        let target = d.target.and_then(|m| self.target_for(m, g));
        let on_old_dialog = d.target.and_then(|m| g.element(m)).is_some_and(|el| {
            self.last_dialog
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains(el)
        });
        if on_old_dialog || matches!(target, Some(MarkTarget::Dialog(_))) {
            anyhow::bail!("that page dialog is already closed; look at the page again");
        }
        // A listed dropdown option, or a native dropdown to open: handled
        // here, never by mouse (the browser's own popup is invisible).
        match (d.action, target) {
            (Action::Click | Action::DoubleClick, Some(MarkTarget::Option { select, index })) => {
                let label = d
                    .target
                    .and_then(|m| g.element(m))
                    .map(|e| option_label(&e.name))
                    .unwrap_or_default();
                return self.choose(select, &label, Some(index)).await;
            }
            (Action::Click, Some(MarkTarget::Dom(r))) if r.select => {
                let _ = self.page("DOM.focus", json!({"backendNodeId": r.id})).await;
                self.open_dropdown(r);
                return Ok(());
            }
            (Action::Wait, _) => return Ok(()),
            (Action::Key, _)
                if d.keys == ["esc"] && self.open_select.lock().is_ok_and(|o| o.is_some()) =>
            {
                self.close_dropdown();
                return Ok(());
            }
            _ => self.close_dropdown(),
        }
        landed(match d.action {
            Action::Click | Action::DoubleClick | Action::RightClick => {
                let mark = d.target.ok_or_else(|| anyhow::anyhow!("no target"))?;
                let (button, times) = match d.action {
                    Action::DoubleClick => ("left", 2),
                    Action::RightClick => ("right", 1),
                    _ => ("left", 1),
                };
                self.click(self.point(mark, g).await?, button, times).await
            }
            Action::Type => match target {
                Some(MarkTarget::Dom(r)) if r.select => {
                    self.choose(r, d.text.as_deref().unwrap_or(""), None).await
                }
                _ => {
                    if let Some(mark) = d.target {
                        self.click(self.point(mark, g).await?, "left", 1).await?;
                        // Typing a value into a field means the field should
                        // hold that value: replace what is there instead of
                        // appending to it (measured live: an e-mail typed
                        // twice failed the form's validation and every click
                        // on Send did nothing).
                        self.press(&["ctrl".to_string(), "a".to_string()]).await?;
                    }
                    let text = d.text.as_deref().unwrap_or("");
                    self.page("Input.insertText", json!({"text": text}))
                        .await
                        .map(drop)
                }
            },
            // Enter after choosing in a native dropdown (the engine sends it)
            // would only open its popup: the choice is already made.
            Action::Key
                if d.keys == ["enter"] && matches!(self.focused_select().await, Some(true)) =>
            {
                Ok(())
            }
            Action::Key => self.press(&d.keys).await,
            Action::Scroll => self.scroll(d, g).await,
            Action::Wait | Action::AskUser | Action::Done => Ok(()),
            Action::OpenApp | Action::Shell | Action::OpenUrl => unreachable!("handled above"),
        })
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

impl BrowserLane {
    /// Whether the focused element is a native `<select>`.
    async fn focused_select(&self) -> Option<bool> {
        let r = self
            .page(
                "Runtime.evaluate",
                json!({
                    "expression": "document.activeElement && document.activeElement.tagName === 'SELECT'",
                    "returnByValue": true,
                }),
            )
            .await
            .ok()?;
        r["result"]["value"].as_bool()
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

    #[test]
    fn option_labels_come_back_from_mark_names() {
        assert_eq!(option_label("Billing (option)"), "Billing");
        assert_eq!(option_label("General (selected option)"), "General");
        assert_eq!(
            option_label("Odd (option) name (option)"),
            "Odd (option) name"
        );
    }

    #[test]
    fn an_input_interrupted_by_a_dialog_counts_as_done() {
        assert!(landed(Err(DialogOpen.into())).is_ok());
        assert!(landed(Err(anyhow::anyhow!("boom"))).is_err());
        assert!(landed(Ok(())).is_ok());
    }
}
