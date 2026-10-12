//! Observation policy: full vs tree-only, the cheap verifier, no-progress
//! detection, and the zoomed crop after a missed grid click.

use super::{Engine, Run, effect};
use crate::element::{Grounding, Rect};
use crate::ports::{EngineEvent, Observation, ObserveTiming};
use crate::schema::{Action, Decision};
use crate::trace::Timings;
use std::time::Instant;

/// Timings for a step whose observation took `observe_ms` in total.
fn observe_timings(t: ObserveTiming, observe_ms: u64) -> Timings {
    if t.capture_ms + t.tree_ms + t.encode_ms > 0 {
        Timings {
            capture_ms: t.capture_ms,
            tree_ms: t.tree_ms,
            encode_ms: t.encode_ms,
            ..Default::default()
        }
    } else {
        Timings {
            capture_ms: observe_ms,
            ..Default::default()
        }
    }
}

/// A Set-of-Marks list at least this long counts as "rich" (tree-only OK).
pub const MIN_RICH_ELEMENTS: usize = 8;
/// Verifier probability at or above which a step counts as verified.
pub const VERIFY_OK: f32 = 0.85;
/// Verifier probability at or below which the step is flagged.
pub const VERIFY_BAD: f32 = 0.15;

/// Words in an `expect` that only pixels can confirm.
const VISUAL_WORDS: &[&str] = &[
    "image",
    "picture",
    "photo",
    "icon",
    "color",
    "colour",
    "video",
    "chart",
    "graph",
    "canvas",
    "highlight",
    "animation",
    "pixel",
    "drawing",
];

/// What one observation round produced.
pub(super) struct Seen {
    pub obs: Observation,
    pub timings: Timings,
}

pub(super) fn rich(g: &Grounding) -> bool {
    g.mark_count() >= MIN_RICH_ELEMENTS
}

/// Can the text of the element list confirm what `action` expected?
pub(super) fn text_checkable(action: Action, expect: &str) -> bool {
    match action {
        // Output is the evidence; the screen is not involved.
        Action::Shell => true,
        Action::Click | Action::DoubleClick | Action::RightClick | Action::Type | Action::Key => {
            let e = expect.to_lowercase();
            !e.trim().is_empty() && !VISUAL_WORDS.iter().any(|w| e.contains(w))
        }
        // New windows/pages, scrolled content and waits need a real look.
        _ => false,
    }
}

/// A click on a dropdown or menu opens a popup the tree describes poorly.
pub(super) fn opens_popup(d: &Decision, g: &Grounding) -> bool {
    use crate::element::Role;
    d.action == Action::Click
        && d.target
            .and_then(|t| g.element(t))
            .is_some_and(|e| matches!(e.role, Role::ComboBox | Role::MenuItem))
}

/// Enter/Return usually submits a form or opens something new.
pub(super) fn submits(keys: &[String]) -> bool {
    keys.iter().any(|k| {
        matches!(
            k.to_ascii_lowercase().as_str(),
            "enter" | "return" | "kp_enter"
        )
    })
}

/// Whether the element text contains (the start of) what was typed.
pub(super) fn tree_shows(tree: &str, typed: &str) -> bool {
    let probe: String = typed.trim().chars().take(24).collect();
    probe.is_empty() || tree.to_lowercase().contains(&probe.to_lowercase())
}

/// The grid cell a click aimed at, to zoom on if it changes nothing.
pub(super) fn grid_click(d: &Decision, g: &Grounding) -> Option<(u32, Rect)> {
    match (g, d.action, d.target) {
        (
            Grounding::Grid { .. },
            Action::Click | Action::DoubleClick | Action::RightClick,
            Some(t),
        ) => g.target_rect(t).map(|r| (t, r)),
        _ => None,
    }
}

/// The clicked cell plus one cell on each side, clamped to the screen.
pub(super) fn zoom_rect(cell: Rect, g: &Grounding) -> Rect {
    let mut r = Rect::new(cell.x - cell.w, cell.y - cell.h, cell.w * 3.0, cell.h * 3.0);
    if let Grounding::Grid { screen, .. } = g {
        let x0 = r.x.max(screen.x);
        let y0 = r.y.max(screen.y);
        let x1 = (r.x + r.w).min(screen.x + screen.w);
        let y1 = (r.y + r.h).min(screen.y + screen.h);
        r = Rect::new(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0));
    }
    r
}

fn text_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

impl Engine<'_> {
    /// One observation, full or tree-only, with any one-time code Kai typed
    /// for the user blanked out of the element list before anything (model,
    /// verifier, trace, UI) reads it.
    pub(super) async fn look(&self, r: &Run<'_>, tree_only: bool) -> anyhow::Result<Observation> {
        let mut obs = if tree_only {
            self.perceptor.observe_tree_only().await?
        } else {
            self.perceptor.observe().await?
        };
        crate::human::redact(&mut obs.grounding, &r.human.codes);
        Ok(obs)
    }

    /// Whether this step may skip the screenshot. Any doubt means no.
    fn want_tree_only(&self, r: &Run<'_>) -> bool {
        let c = &self.config;
        c.tree_only
            && !r.force_full
            && r.last_rich
            && r.failure_streak == 0
            && r.no_progress_streak == 0
            && r.tree_only_streak + 1 < c.full_every
            && r.pending.as_ref().is_some_and(|p| {
                text_checkable(p.action, &p.expect) && !submits(&p.keys) && !p.popup
            })
    }

    /// Observe (full or tree-only), verify the last `expect`, update the
    /// no-progress state, and add the zoom crop after a missed grid click.
    /// `exec`: an observation for the executor, which reads text only: tree
    /// only whenever tree-only steps are allowed, and no verifier (the
    /// executor checks `expect` itself).
    pub(super) async fn observe_step(&self, r: &mut Run<'_>, exec: bool) -> anyhow::Result<Seen> {
        let step = r.step;
        let t0 = Instant::now();
        let mut tree_only = if exec {
            self.config.tree_only
        } else {
            self.want_tree_only(r)
        };
        let mut obs = self.look(r, tree_only).await?;
        // Typed text that the tree doesn't show (web pages often don't expose
        // field values) would read as "typing failed": look properly.
        let typed_hidden = tree_only
            && r.pending
                .as_ref()
                .and_then(|p| p.typed.as_deref())
                .is_some_and(|t| !tree_shows(&obs.grounding.as_text(), t));
        if tree_only && (typed_hidden || !rich(&obs.grounding)) {
            // The tree got poor (or turned into a grid): look properly.
            obs = self.look(r, false).await?;
            tree_only = false;
        }

        // ---- Cheap verifier on the element text ----
        let mut verify_ms = 0;
        let mut p = None;
        if let (Some(v), Some(pend), false) = (self.verifier, r.pending.as_ref(), exec)
            && !pend.expect.trim().is_empty()
        {
            let state = format!(
                "Last action: {}\nElements now:\n{}",
                pend.summary,
                obs.grounding.as_compact_text()
            );
            let tv = Instant::now();
            p = v.check(r.task, &pend.expect, &state).await;
            verify_ms = tv.elapsed().as_millis() as u64;
            if let Some(p) = p
                && p <= VERIFY_BAD
            {
                r.careful_next = true;
                r.notes.push(format!(
                    "Check: the expected result of your last action (\"{}\") does not seem \
                     to hold. Look carefully before going on.",
                    pend.expect
                ));
                if tree_only {
                    obs = self.look(r, false).await?;
                    tree_only = false;
                }
            }
        }
        let observe_ms = (t0.elapsed().as_millis() as u64).saturating_sub(verify_ms);

        // ---- No-progress: same screen (or same tree on tree-only steps),
        // and no accessibility event since the action (stage 2) ----
        let th = text_hash(&obs.grounding.as_text());
        let same = match r.last_sig {
            Some((_, prev_th, was_tree)) if was_tree || tree_only => prev_th == th,
            // Both the pixels and the element list must be unchanged.
            Some((h, prev_th, _)) => h == obs.screen_hash && prev_th == th,
            None => false,
        };
        let fx = effect::Seen {
            list_changed: r.last_sig.is_some_and(|(_, prev_th, _)| prev_th != th),
            same,
            events: effect::events_since(r.events_mark.take(), self.perceptor.ui_events()),
        };
        let no_progress = fx.no_op();
        if no_progress {
            r.no_progress_streak += 1;
        } else {
            r.no_progress_streak = 0;
        }
        if let Some(pend) = r.pending.take() {
            let ok = match p {
                Some(p) if p <= VERIFY_BAD => false,
                Some(p) if p >= VERIFY_OK => true,
                _ => !no_progress || matches!(pend.action, Action::Shell | Action::Wait),
            };
            let mut labelled = Vec::new();
            for i in pend.records {
                if let Some(s) = r.trace.steps.get_mut(i) {
                    s.verified = ok;
                    let e = effect::label(s.decision.action, s.output.as_deref(), fx);
                    s.effect = Some(e);
                    labelled.push((s.index, e));
                }
            }
            for (idx, e) in labelled {
                self.ledger_effect(r, idx, e);
            }
        }

        // ---- Zoomed crop after a grid click that changed nothing ----
        if no_progress
            && let Some((cell, rect)) = r.grid_click.take()
            && let Some(crop) = self.perceptor.zoom(zoom_rect(rect, &obs.grounding)).await
        {
            obs.images.crop_jpeg = Some(crop);
            r.notes.push(format!(
                "Your click on cell {cell} changed nothing. The second image is a zoom of \
                 cell {cell} (centre) and its neighbours; pick the cell that really holds \
                 the target."
            ));
        }

        r.last_sig = Some((obs.screen_hash, th, tree_only));
        r.last_rich = rich(&obs.grounding);
        if tree_only {
            r.tree_only_streak += 1;
        } else {
            r.tree_only_streak = 0;
            r.force_full = false;
        }

        self.sink.emit(&EngineEvent::Observed {
            step,
            elements: obs.grounding.as_text(),
            screenshot: obs.images.screenshot_jpeg.as_slice().into(),
            screen: obs.screen,
        });
        let mut timings = observe_timings(obs.timing, observe_ms);
        timings.verify_ms = verify_ms;
        Ok(Seen { obs, timings })
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn enter_submits_and_typed_text_is_looked_for() {
        assert!(submits(&["Return".to_string()]));
        assert!(!submits(&["ctrl".to_string(), "s".to_string()]));
        assert!(tree_shows(
            "[3] entry \"Gemma 4 model card\"",
            "gemma 4 model card"
        ));
        assert!(!tree_shows("[3] entry \"Search\"", "Gemma 4 model card"));
    }

    use super::*;

    #[test]
    fn visual_expectations_need_a_screenshot() {
        assert!(text_checkable(Action::Click, "Save dialog opens"));
        assert!(!text_checkable(Action::Click, "the image is rotated"));
        assert!(!text_checkable(Action::Click, ""));
        assert!(!text_checkable(Action::OpenUrl, "page loads"));
        assert!(text_checkable(Action::Shell, ""));
    }

    #[test]
    fn zoom_rect_is_three_cells_clamped() {
        let g = Grounding::Grid {
            cols: 4,
            rows: 4,
            screen: Rect::new(0.0, 0.0, 400.0, 400.0),
        };
        // Cell 6 is index 5: column 1, row 1.
        let c = g.target_rect(6).unwrap();
        assert_eq!(zoom_rect(c, &g), Rect::new(0.0, 0.0, 300.0, 300.0));
        let corner = g.target_rect(1).unwrap();
        assert_eq!(zoom_rect(corner, &g), Rect::new(0.0, 0.0, 200.0, 200.0));
    }
}
