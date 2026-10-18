//! A model going round in circles: the same reasoning step after step while
//! its clicks land somewhere new each time, so the screen changes and the
//! no-progress and A-B-A-B guards stay quiet. Seen live (Gemma 4 E4B, a
//! save dialog read as a grid): four steps of "the current screen is a file
//! explorer, I need to open the text editor" while the editor was open
//! behind the dialog.

use crate::trace::StepRecord;

/// Words of a thought compared; thoughts are at most 25 words.
const WORDS: usize = 12;

/// What the model hears when it repeats itself.
pub(super) const NOTE: &str = "Your last steps repeated the same reasoning and did not get \
you closer. Stop and look at the screen again: which window is in front, and what have \
your steps already done? Then take a different route.";

/// The first [`WORDS`] words of `thought`, lowercase, punctuation dropped.
fn gist(thought: &str) -> Vec<String> {
    thought
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .take(WORDS)
        .collect()
}

/// Whether `thought` repeats the last model step's thought (steps without
/// one, such as batched follow-ups, are skipped). A short thought says too
/// little to count.
pub(super) fn repeats(steps: &[StepRecord], thought: &str) -> bool {
    let now = gist(thought);
    if now.len() < WORDS / 2 {
        return false;
    }
    steps
        .iter()
        .rev()
        .map(|s| gist(&s.decision.thought))
        .find(|g| !g.is_empty())
        .is_some_and(|before| before == now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Decision;
    use crate::trace::Timings;

    fn step(thought: &str) -> StepRecord {
        let d = Decision::parse_checked(
            &serde_json::json!({"thought": thought, "action": "wait", "risk": "none"}).to_string(),
        )
        .unwrap();
        StepRecord::new(0, &d, Timings::default(), "test")
    }

    const SEEN: &str = "The current screen is a file explorer. I need to find and open the \
                        text editor, which is likely Mousepad.";

    #[test]
    fn the_same_reasoning_again_is_a_repeat() {
        assert!(repeats(&[step(SEEN)], SEEN));
        // Punctuation, case and the tail after twelve words do not matter.
        let again = "the current screen is a file explorer, i need to find and open it";
        assert!(repeats(&[step(SEEN)], again));
        // A batched follow-up without a thought sits between them.
        assert!(repeats(&[step(SEEN), step("")], SEEN));
    }

    #[test]
    fn new_reasoning_or_a_short_thought_is_not() {
        assert!(!repeats(&[], SEEN));
        assert!(!repeats(
            &[step(SEEN)],
            "The save dialog is open: type the file name into the Name field."
        ));
        assert!(!repeats(&[step("click next")], "click next"));
    }
}
