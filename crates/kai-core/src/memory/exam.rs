//! The memory exam (`docs/MEMORY.md`, "Consolidation and the exam"): a new
//! set of lessons goes live only if it beats the current one on a paired
//! benchmark. The same tasks run once with the current lessons and once
//! with the current plus the pending ones. The candidate passes when it wins
//! at least `margin` more tasks than it loses and says `done` falsely no more
//! often. OS-free: the caller reads the two bench result files.

use serde::Serialize;

/// The default margin: wins minus losses.
pub const MARGIN: i64 = 4;

/// One task in both runs: passed with the current lessons, passed with the
/// candidate set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paired {
    pub task: String,
    pub current: bool,
    pub candidate: bool,
}

/// What the exam found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub tasks: usize,
    /// Tasks only the candidate passed, and only the current set passed.
    pub wins: Vec<String>,
    pub losses: Vec<String>,
    pub false_done_current: u32,
    pub false_done_candidate: u32,
    pub margin: i64,
    pub pass: bool,
}

/// Judge a paired run. Tasks that are in only one run do not count.
pub fn judge(pairs: &[Paired], false_done: (u32, u32), margin: i64) -> Verdict {
    let wins: Vec<String> = pairs
        .iter()
        .filter(|p| p.candidate && !p.current)
        .map(|p| p.task.clone())
        .collect();
    let losses: Vec<String> = pairs
        .iter()
        .filter(|p| p.current && !p.candidate)
        .map(|p| p.task.clone())
        .collect();
    let net = wins.len() as i64 - losses.len() as i64;
    Verdict {
        tasks: pairs.len(),
        pass: net >= margin && false_done.1 <= false_done.0,
        wins,
        losses,
        false_done_current: false_done.0,
        false_done_candidate: false_done.1,
        margin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(spec: &[(bool, bool)]) -> Vec<Paired> {
        spec.iter()
            .enumerate()
            .map(|(i, (a, b))| Paired {
                task: format!("t{i}"),
                current: *a,
                candidate: *b,
            })
            .collect()
    }

    #[test]
    fn passes_only_with_a_clear_net_win() {
        let four_up = pairs(&[(false, true); 4]);
        assert!(judge(&four_up, (0, 0), MARGIN).pass);
        let mut mixed = pairs(&[(false, true); 5]);
        mixed.extend(pairs(&[(true, false); 2]));
        let v = judge(&mixed, (0, 0), MARGIN);
        assert_eq!((v.wins.len(), v.losses.len(), v.pass), (5, 2, false));
    }

    #[test]
    fn more_false_done_fails_whatever_the_wins() {
        let up = pairs(&[(false, true); 8]);
        assert!(!judge(&up, (1, 2), MARGIN).pass);
        assert!(judge(&up, (2, 1), MARGIN).pass);
    }

    #[test]
    fn ties_and_no_change_fail() {
        assert!(!judge(&pairs(&[(true, true); 10]), (0, 0), MARGIN).pass);
        assert!(!judge(&[], (0, 0), MARGIN).pass);
    }
}
