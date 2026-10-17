//! Best-of-K for one step (`--step-candidates K`): K sampled replies, grouped
//! by what they would do, the largest group wins and only its first reply's
//! action runs (`docs/RESEARCH_2026_10_OPENAI_PARITY.md` §5d).
//! Exposes all tied groups so an optional model can judge between them.
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

/// Each group contains distinct candidate indices agreeing with one another.
/// The first group wins deterministic ties; a model may choose another group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vote {
    pub groups: Vec<Vec<usize>>,
}

impl Vote {
    pub fn winner(&self) -> usize {
        self.groups[0][0]
    }
}

/// The largest non-overlapping agreeing group(s). Evaluate every subset of up
/// to eight candidates: greedy first-fit can miss the actual majority when a
/// candidate agrees with several partially overlapping groups. Pairwise
/// agreement prevents chains of nearby points joining distant endpoints.
pub fn vote<'a>(turns: impl IntoIterator<Item = &'a Turn>) -> Option<Vote> {
    let turns: Vec<&Turn> = turns.into_iter().collect();
    if turns.is_empty() || turns.len() > 8 {
        return None;
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut remaining = (1usize << turns.len()) - 1;
    while remaining != 0 {
        let mut best: Vec<usize> = Vec::new();
        for subset in 1..(1usize << turns.len()) {
            if subset & !remaining != 0 || (subset.count_ones() as usize) < best.len() {
                continue;
            }
            let indices = (0..turns.len())
                .filter(|&i| subset & (1 << i) != 0)
                .collect::<Vec<_>>();
            if (indices.len() > best.len() || indices < best)
                && indices
                    .iter()
                    .enumerate()
                    .all(|(a, &i)| indices[a + 1..].iter().all(|&j| agrees(turns[i], turns[j])))
            {
                best = indices;
            }
        }
        debug_assert!(!best.is_empty());
        for &i in &best {
            remaining &= !(1 << i);
        }
        groups.push(best);
    }
    let largest = groups.iter().map(Vec::len).max().unwrap_or(0);
    let tied = groups.into_iter().filter(|g| g.len() == largest).collect();
    Some(Vote { groups: tied })
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
        assert_eq!(vote(&turns).unwrap().winner(), 0);
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
        assert_eq!(vote(&[m(3), m(7), m(7)]).unwrap().winner(), 1);
        assert_eq!(vote(&[done, m(3)]).unwrap().winner(), 0);
        assert_eq!(vote(&[]), None);
    }

    #[test]
    fn adjacent_points_across_a_grid_boundary_agree() {
        let turns = [
            click(point(100., 100.)),
            click(point(503., 300.)),
            click(point(504., 300.)),
        ];
        assert_eq!(vote(&turns).unwrap().winner(), 1);
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
            Some(Vote {
                groups: vec![vec![1, 2]]
            })
        );
    }

    #[test]
    fn tie_exposes_distinct_clusters_and_all_risky_members() {
        let t = [
            click(point(10., 10.)),
            click(point(90., 90.)),
            click(point(12., 11.)),
            click(point(91., 89.)),
        ];
        assert_eq!(
            vote(&t),
            Some(Vote {
                groups: vec![vec![0, 2], vec![1, 3]]
            })
        );
    }

    #[test]
    fn overlapping_neighborhoods_do_not_double_count_a_candidate() {
        // 0 and 22 are too far apart even though each agrees with 11.
        let turns = [
            click(point(0., 5.)),
            click(point(11., 5.)),
            click(point(22., 5.)),
        ];
        let result = vote(&turns).unwrap();
        assert_eq!(result.groups, vec![vec![0, 1]]);
        assert_eq!(result.winner(), 0);
    }

    #[test]
    fn every_candidate_in_a_cluster_is_within_the_radius_of_every_other() {
        let turns = [
            click(point(0., 0.)),
            click(point(10., 0.)),
            click(point(20., 0.)),
            click(point(30., 0.)),
        ];
        let result = vote(&turns).unwrap();
        assert_eq!(result.groups, vec![vec![0, 1], vec![2, 3]]);
    }

    #[test]
    fn largest_pairwise_cluster_wins_regardless_of_initial_order() {
        // First-fit produces [0, 11] and [22, 21] (a false tie). The true
        // majority [11, 22, 21] must win without counting candidates twice.
        let turns = [
            click(point(0., 5.)),
            click(point(11., 5.)),
            click(point(22., 5.)),
            click(point(21., 5.)),
        ];
        assert_eq!(vote(&turns).unwrap().groups, vec![vec![1, 2, 3]]);
    }

    #[test]
    fn excessive_candidates_fail_closed() {
        let turns = (0..9).map(|_| click(point(1., 1.))).collect::<Vec<_>>();
        assert_eq!(vote(&turns), None);
    }
}
