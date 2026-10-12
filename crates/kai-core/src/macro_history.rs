//! Macro-action history (`docs/RESEARCH_2026_10_FOUNDATIONS.md` §2.4).
//!
//! With `Config::macro_history`, the turn no longer carries only the last 3
//! raw steps: everything older than the last [`RAW_STEPS`] is folded into one
//! running summary of at most [`MAX_WORDS`] words, verified outcomes first.
//! It is built from the history in code, with no model call, so the model
//! keeps sight of what it already did on long runs.

use crate::prompt::HistoryItem;

/// Raw steps shown in full after the summary.
pub const RAW_STEPS: usize = 2;
/// Longest the summary may be, verified outcomes included.
pub const MAX_WORDS: usize = 60;
/// Longest one step may read in the summary.
const STEP_WORDS: usize = 8;

/// The summary line for the steps before the raw window, or `None` when
/// there is nothing to say. `verified` holds the descriptions of outcomes
/// the ledger has verified (already redacted).
pub fn summary(older: &[HistoryItem], verified: &[String]) -> Option<String> {
    let mut words = 0;
    let mut out = String::new();
    if !verified.is_empty() {
        // At most half the budget, so the steps always have room.
        let v = cap_words(&verified.join("; "), MAX_WORDS / 2 - 1);
        words += v.split_whitespace().count() + 1;
        out.push_str("Verified: ");
        out.push_str(&v);
        out.push('.');
    }

    let steps = merge(older.iter().map(phrase).collect());
    // Newest first until the budget runs out; what does not fit is counted.
    let mut kept: Vec<&str> = Vec::new();
    for p in steps.iter().rev() {
        let n = p.split_whitespace().count();
        if words + n + 3 > MAX_WORDS {
            break;
        }
        words += n;
        kept.push(p);
    }
    kept.reverse();
    let dropped = steps.len() - kept.len();
    if !kept.is_empty() || dropped > 0 {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str("Earlier: ");
        if dropped > 0 {
            out.push_str(&format!("({dropped} older) "));
        }
        out.push_str(&kept.join("; "));
        out.push('.');
    }
    (!out.is_empty()).then_some(out)
}

/// One step in a few words. Mark numbers change every frame, so `#14`
/// becomes the element's name when it was known, and is dropped otherwise.
fn phrase(h: &HistoryItem) -> String {
    let mut words: Vec<String> = Vec::new();
    for w in h.summary.split_whitespace() {
        if is_mark(w) {
            if let Some(name) = h.target_name.as_deref().filter(|n| !n.trim().is_empty()) {
                words.push(format!("\"{}\"", cap_words(name.trim(), 3)));
            }
        } else {
            words.push(w.to_string());
        }
    }
    let mut p = cap_words(&words.join(" "), STEP_WORDS);
    if let Some(code) = h.output.as_deref().and_then(exit_code) {
        p.push_str(&format!(" [exit {code}]"));
    }
    p
}

fn is_mark(w: &str) -> bool {
    w.len() > 1 && w.starts_with('#') && w[1..].chars().all(|c| c.is_ascii_digit())
}

/// The last `[exit N]` in a command's output.
fn exit_code(out: &str) -> Option<&str> {
    let i = out.rfind("[exit ")?;
    let rest = &out[i + 6..];
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '-')?;
    (end > 0).then(|| &rest[..end])
}

/// Repeats in a row become one entry: "scroll down ×3".
fn merge(phrases: Vec<String>) -> Vec<String> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for p in phrases {
        match out.last_mut() {
            Some((last, n)) if *last == p => *n += 1,
            _ => out.push((p, 1)),
        }
    }
    out.into_iter()
        .map(|(p, n)| if n > 1 { format!("{p} ×{n}") } else { p })
        .collect()
}

fn cap_words(s: &str, max: usize) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() <= max {
        words.join(" ")
    } else {
        format!("{}…", words[..max].join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(s: &str) -> HistoryItem {
        HistoryItem::note(s)
    }

    #[test]
    fn nothing_to_say_is_none() {
        assert_eq!(summary(&[], &[]), None);
    }

    #[test]
    fn marks_become_names_or_vanish() {
        let mut named = step("click #14");
        named.target_name = Some("Save".into());
        let s = summary(&[named, step("click #3")], &[]).unwrap();
        assert_eq!(s, "Earlier: click \"Save\"; click.");
        assert!(!s.contains('#'));
    }

    #[test]
    fn shell_steps_keep_their_exit_code() {
        let mut h = step("shell \"ls ~/Downloads\"");
        h.push_output("a.txt\nb.txt\n[exit 0]");
        let mut bad = step("shell \"cat nope\"");
        bad.push_output("cat: nope: No such file\n[exit 1]");
        let s = summary(&[h, bad], &[]).unwrap();
        assert!(s.contains("[exit 0]") && s.contains("[exit 1]"), "{s}");
        assert!(!s.contains("b.txt"), "outputs stay out of the summary: {s}");
    }

    #[test]
    fn repeats_are_merged() {
        let s = summary(
            &[
                step("scroll down"),
                step("scroll down"),
                step("scroll down"),
            ],
            &[],
        );
        assert_eq!(s.unwrap(), "Earlier: scroll down ×3.");
    }

    #[test]
    fn verified_outcomes_come_first() {
        let s = summary(&[step("click #2")], &["file saved".into()]).unwrap();
        assert!(s.starts_with("Verified: file saved."), "{s}");
    }

    #[test]
    fn never_longer_than_the_budget_and_counts_what_it_drops() {
        let older: Vec<HistoryItem> = (0..40)
            .map(|i| step(&format!("type #{i} \"word{i} more text here please\"")))
            .collect();
        let verified = vec!["the report was written to the shared folder".to_string()];
        let s = summary(&older, &verified).unwrap();
        assert!(s.split_whitespace().count() <= MAX_WORDS, "{s}");
        assert!(s.contains("older)"), "{s}");
        // The newest older step survives; the oldest does not.
        assert!(s.contains("word39") && !s.contains("word0 "), "{s}");
    }

    #[test]
    fn a_huge_verified_list_is_capped_too() {
        let verified = vec!["x ".repeat(200)];
        let s = summary(&[step("click #1")], &verified).unwrap();
        assert!(s.split_whitespace().count() <= MAX_WORDS, "{s}");
        assert!(s.contains("Earlier:"), "{s}");
    }
}
