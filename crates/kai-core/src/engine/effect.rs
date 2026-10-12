//! Per-action effect (stage 2 of `docs/VERIFICATION.md`): after an action,
//! the next observation labels it `confirmed`, `unverifiable` or `no_op`
//! from the element-list diff, accessibility events from the active app and
//! the screen hash, with no model call. `no_op` is the no-progress signal;
//! without events it is exactly the old "same list and same pixels" rule.

use crate::ledger::Effect;
use crate::schema::Action;

/// What the next observation saw after the action.
#[derive(Debug, Clone, Copy)]
pub(super) struct Seen {
    /// The element list text differs from the last observation.
    pub list_changed: bool,
    /// The list is the same and, on full frames, so are the pixels.
    pub same: bool,
    /// Accessibility events since the action; `None` when unavailable.
    pub events: Option<u64>,
}

impl Seen {
    /// Counts as no progress: nothing visible changed and no event arrived.
    pub fn no_op(&self) -> bool {
        self.same && !self.events.is_some_and(|n| n > 0)
    }
}

/// Label one action. `output` is a shell step's output.
pub(super) fn label(action: Action, output: Option<&str>, seen: Seen) -> Effect {
    match action {
        // The exit code is the evidence; the screen is not involved.
        Action::Shell if output.is_some_and(|o| o.trim_end().ends_with("[exit 0]")) => {
            Effect::Confirmed
        }
        Action::Shell | Action::Wait => Effect::Unverifiable,
        _ if seen.list_changed || seen.events.is_some_and(|n| n > 0) => Effect::Confirmed,
        _ if seen.no_op() => Effect::NoOp,
        _ => Effect::Unverifiable,
    }
}

/// Events between the mark taken before the action and now.
pub(super) fn events_since(mark: Option<u64>, now: Option<u64>) -> Option<u64> {
    match (mark, now) {
        (Some(a), Some(b)) => Some(b.saturating_sub(a)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(list_changed: bool, same: bool, events: Option<u64>) -> Seen {
        Seen {
            list_changed,
            same,
            events,
        }
    }

    #[test]
    fn labels() {
        use Effect::*;
        // Without events: the old rule.
        assert_eq!(label(Action::Click, None, seen(false, true, None)), NoOp);
        assert_eq!(
            label(Action::Click, None, seen(true, false, None)),
            Confirmed
        );
        assert_eq!(
            label(Action::Click, None, seen(false, false, None)),
            Unverifiable
        );
        // Events rescue an invisible change, and zero events changes nothing.
        assert_eq!(
            label(Action::Click, None, seen(false, true, Some(2))),
            Confirmed
        );
        assert_eq!(label(Action::Click, None, seen(false, true, Some(0))), NoOp);
        assert!(!seen(false, true, Some(1)).no_op());
        // Shell and wait.
        assert_eq!(
            label(Action::Shell, Some("ok\n[exit 0]"), seen(false, true, None)),
            Confirmed
        );
        assert_eq!(
            label(Action::Shell, Some("[exit 2]"), seen(true, false, None)),
            Unverifiable
        );
        assert_eq!(
            label(Action::Wait, None, seen(true, false, Some(5))),
            Unverifiable
        );
        assert_eq!(events_since(Some(3), Some(7)), Some(4));
        assert_eq!(events_since(None, Some(7)), None);
    }
}
