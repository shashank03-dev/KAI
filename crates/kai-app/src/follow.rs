//! Transcript scrolling: a run's Steps and Terminal views follow the newest
//! entry until the user scrolls up to read older ones. Then they hold the
//! user's place, count what arrives, and offer "Jump to latest"; scrolling
//! back to the end, or the jump, follows again.
//!
//! The policy is pure (offsets in, a decision out) so it is tested without a
//! window; [`Transcripts`] keeps one scroll handle and policy per run and tab.

use gpui::ScrollHandle;
use std::collections::HashMap;

/// Scrolling down to within this many pixels of the end follows again, so
/// a wheel tick that stops just short counts as reaching the latest.
pub const END_SLACK: f32 = 24.0;

/// GPUI rounds the scroll range to hundredths; this much off the end is
/// still the end.
const AT_END: f32 = 1.0;

/// What one frame's layout says about the view. `offset` is GPUI's: 0 at the
/// top, negative further down, never below `-max`.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub offset: f32,
    pub max: f32,
    pub entries: usize,
}

impl Frame {
    fn within(&self, slack: f32) -> bool {
        -self.offset >= self.max - slack
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Follow {
    following: bool,
    /// Entries shown while following, or when the user last returned.
    seen: usize,
    /// The offset at the previous frame: a change is the user's doing
    /// (wheel, scrollbar, keyboard focus) or a jump this policy asked for.
    last: Option<f32>,
}

impl Default for Follow {
    fn default() -> Self {
        Self {
            following: true,
            seen: 0,
            last: None,
        }
    }
}

impl Follow {
    /// Called once per render; true when the view should scroll to its end.
    /// Content that grows under a still offset keeps following. Any move up
    /// off the end stops it, however small (a trackpad sends a few pixels
    /// at a time); a move down to near the end follows again.
    pub fn frame(&mut self, f: Frame) -> bool {
        if let Some(last) = self.last.filter(|last| *last != f.offset) {
            let up = f.offset > last;
            self.following = f.within(if up { AT_END } else { END_SLACK });
        }
        self.last = Some(f.offset);
        if self.following || f.entries < self.seen {
            self.seen = f.entries;
        }
        self.following
    }

    /// The user asked for the latest entry.
    pub fn jump(&mut self, entries: usize) {
        self.following = true;
        self.seen = entries;
    }

    #[cfg(test)]
    fn following(&self) -> bool {
        self.following
    }

    /// Entries that arrived while the user read older ones.
    pub fn unseen(&self, entries: usize) -> usize {
        if self.following {
            0
        } else {
            entries.saturating_sub(self.seen)
        }
    }
}

/// "Jump to latest", with how many entries arrived meanwhile.
pub fn jump_label(unseen: usize) -> String {
    match unseen {
        0 => "Jump to latest".into(),
        1 => "Jump to latest · 1 new".into(),
        n => format!("Jump to latest · {n} new"),
    }
}

/// One scroll position and follow policy per run and tab, so switching tabs
/// or tasks keeps each place.
#[derive(Default)]
pub struct Transcripts {
    views: HashMap<(u64, String), (ScrollHandle, Follow)>,
}

impl Transcripts {
    pub fn view(&mut self, run: u64, tab: &str) -> &mut (ScrollHandle, Follow) {
        self.views.entry((run, tab.to_string())).or_default()
    }

    /// Forget runs that left the task list.
    pub fn retain(&mut self, keep: impl Fn(u64) -> bool) {
        self.views.retain(|(run, _), _| keep(*run));
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.views.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(offset: f32, max: f32, entries: usize) -> Frame {
        Frame {
            offset,
            max,
            entries,
        }
    }

    #[test]
    fn follows_new_entries_while_at_the_end() {
        let mut f = Follow::default();
        assert!(f.frame(at(0.0, 0.0, 1)));
        // Content grew under a still offset: keep following.
        assert!(f.frame(at(0.0, 300.0, 2)));
        // The jump landed at the end.
        assert!(f.frame(at(-300.0, 300.0, 3)));
        assert_eq!(f.unseen(3), 0);
    }

    #[test]
    fn scrolling_up_holds_the_place_and_counts_arrivals() {
        let mut f = Follow::default();
        f.frame(at(-500.0, 500.0, 4));
        assert!(!f.frame(at(-200.0, 500.0, 4)), "user scrolled up");
        assert!(!f.frame(at(-200.0, 800.0, 6)), "new steps must not yank");
        assert_eq!(f.unseen(6), 2);
        assert_eq!(jump_label(f.unseen(6)), "Jump to latest · 2 new");
    }

    #[test]
    fn jump_returns_to_following() {
        let mut f = Follow::default();
        f.frame(at(-500.0, 500.0, 4));
        f.frame(at(-100.0, 500.0, 5));
        f.jump(5);
        assert!(f.frame(at(-100.0, 500.0, 5)));
        assert!(f.following());
        assert_eq!(f.unseen(7), 0);
    }

    #[test]
    fn scrolling_back_to_the_end_follows_again() {
        let mut f = Follow::default();
        f.frame(at(-500.0, 500.0, 4));
        f.frame(at(-100.0, 500.0, 4));
        assert!(f.frame(at(-490.0, 500.0, 5)), "within the slack");
        assert_eq!(f.unseen(5), 0);
    }

    #[test]
    fn a_small_scroll_up_stops_following() {
        let mut f = Follow::default();
        f.frame(at(-500.0, 500.0, 4));
        assert!(!f.frame(at(-495.0, 500.0, 4)), "a trackpad nudge up");
        assert!(!f.frame(at(-500.0 + END_SLACK + 1.0, 500.0, 4)));
        assert!(
            f.frame(at(-500.0 + END_SLACK, 500.0, 4)),
            "down to near the end"
        );
    }

    #[test]
    fn a_shrinking_range_at_the_end_keeps_following() {
        let mut f = Follow::default();
        f.frame(at(-500.0, 500.0, 4));
        // The window grew: GPUI clamps the offset up to the new end.
        assert!(f.frame(at(-300.0, 300.0, 4)));
    }

    #[test]
    fn a_resize_while_following_stays_pinned() {
        let mut f = Follow::default();
        f.frame(at(-500.0, 500.0, 4));
        // The window got shorter: more to scroll, offset unchanged.
        assert!(f.frame(at(-500.0, 700.0, 4)));
    }

    #[test]
    fn content_that_fits_follows() {
        let mut f = Follow::default();
        f.frame(at(-300.0, 500.0, 4));
        f.frame(at(-100.0, 500.0, 4));
        // The window grew until everything fits; GPUI clamps to 0.
        assert!(f.frame(at(0.0, 0.0, 4)));
    }

    #[test]
    fn fewer_entries_never_underflow() {
        let mut f = Follow::default();
        f.frame(at(-500.0, 500.0, 9));
        f.frame(at(-100.0, 500.0, 9));
        f.frame(at(-100.0, 500.0, 3));
        assert_eq!(f.unseen(3), 0);
        assert_eq!(f.unseen(4), 1);
    }

    #[test]
    fn views_are_kept_per_run_and_tab_and_pruned() {
        let mut t = Transcripts::default();
        t.view(1, "steps").1.frame(at(-500.0, 500.0, 4));
        t.view(1, "steps").1.frame(at(-100.0, 500.0, 4));
        assert!(t.view(1, "terminal").1.following());
        assert!(!t.view(1, "steps").1.following(), "steps kept its place");
        t.view(2, "steps");
        t.retain(|run| run == 2);
        assert_eq!(t.len(), 1);
    }
}
