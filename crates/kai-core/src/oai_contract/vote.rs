//! Best-of-K for one step (`--step-candidates K`): K sampled replies, grouped
//! by what they would do, the largest group wins and only its first reply's
//! action runs (GTA1's step-level scaling with a non-model judge,
//! `docs/RESEARCH_2026_10_OPENAI_PARITY.md` §5d). Ties go to the earliest.
use super::Turn;
use crate::computer::{ComputerAction, Target};

/// Points this close (image pixels) aim at the same thing.
const RADIUS: f64 = 12.0;

/// What a turn would do, coarse enough that agreeing replies match.
fn key(turn: &Turn) -> String {
    let Turn::Act { actions, .. } = turn else {
        return match turn {
            Turn::Done { .. } => "done".into(),
            _ => "ask".into(),
        };
    };
    let Some(first) = actions.first() else {
        return "none".into();
    };
    let at = |t: &Target| match t {
        Target::Element { mark, .. } => format!("#{mark}"),
        Target::Point { .. } => "@".to_string(),
    };
    match first {
        ComputerAction::Click {
            target,
            button,
            count,
            ..
        } => {
            format!(
                "click {button:?} {count} {:?} {}",
                first.modifiers(),
                at(target)
            )
        }
        ComputerAction::Move { target, .. } => {
            format!("move {:?} {}", first.modifiers(), at(target))
        }
        ComputerAction::Scroll { target, dx, dy, .. } => {
            format!(
                "scroll {:?} {} {} {}",
                first.modifiers(),
                at(target),
                dx.signum(),
                dy.signum()
            )
        }
        ComputerAction::Type { target, text } => format!("type {} {text}", at(target)),
        ComputerAction::Keypress { keys } => format!("keys {}", keys.join("+")),
        other => serde_json::to_string(other).expect("computer action serializes"),
    }
}

fn target(turn: &Turn) -> Option<&Target> {
    let Turn::Act { actions, .. } = turn else {
        return None;
    };
    match actions.first()? {
        ComputerAction::Click { target, .. }
        | ComputerAction::Move { target, .. }
        | ComputerAction::Scroll { target, .. }
        | ComputerAction::Type { target, .. } => Some(target),
        _ => None,
    }
}

fn agrees(a: &Turn, b: &Turn) -> bool {
    if key(a) != key(b) {
        return false;
    }
    match (target(a), target(b)) {
        (Some(Target::Point { x: ax, y: ay, .. }), Some(Target::Point { x: bx, y: by, .. })) => {
            (ax - bx).hypot(ay - by) <= RADIUS
        }
        (Some(Target::Element { mark: a, .. }), Some(Target::Element { mark: b, .. })) => a == b,
        (None, None) => true,
        _ => false,
    }
}

/// The index of the winning reply among `turns`; `None` when empty.
pub fn vote<'a>(turns: impl IntoIterator<Item = &'a Turn>) -> Option<usize> {
    let turns: Vec<&Turn> = turns.into_iter().collect();
    (0..turns.len()).max_by_key(|&i| {
        let votes = turns.iter().filter(|t| agrees(turns[i], t)).count();
        // More votes first; among equals, the earliest reply.
        (votes, std::cmp::Reverse(i))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer::MouseButton;

    fn click(target: Target) -> Turn {
        Turn::Act {
            thought: String::new(),
            actions: vec![ComputerAction::Click {
                target,
                button: MouseButton::Left,
                count: 1,
                modifiers: vec![],
            }],
        }
    }
    fn point(x: f64, y: f64) -> Target {
        Target::Point {
            observation: 1,
            x,
            y,
        }
    }

    #[test]
    fn the_majority_wins_and_near_points_agree() {
        let turns = [
            click(point(500., 300.)),
            click(point(100., 100.)),
            click(point(503., 302.)),
            click(point(506., 298.)),
        ];
        assert_eq!(vote(&turns), Some(0));
    }

    #[test]
    fn marks_and_messages_are_grouped_and_ties_go_to_the_first() {
        let m = |mark| {
            click(Target::Element {
                observation: 1,
                mark,
            })
        };
        let done = Turn::Done {
            message: "ok".into(),
        };
        assert_eq!(vote(&[m(3), m(7), m(7)]), Some(1));
        assert_eq!(vote(&[done, m(3)]), Some(0));
        assert_eq!(vote(&[]), None);
    }

    #[test]
    fn adjacent_points_across_a_grid_boundary_agree() {
        let turns = [
            click(point(100., 100.)),
            click(point(503., 300.)),
            click(point(504., 300.)),
        ];
        assert_eq!(vote(&turns), Some(1));
    }

    #[test]
    fn different_urls_are_not_one_vote() {
        let url = |s: &str| Turn::Act {
            thought: String::new(),
            actions: vec![ComputerAction::OpenUrl { url: s.into() }],
        };
        assert_eq!(
            vote(&[
                url("https://a.example"),
                url("https://b.example"),
                url("https://b.example")
            ]),
            Some(1)
        );
    }
}
