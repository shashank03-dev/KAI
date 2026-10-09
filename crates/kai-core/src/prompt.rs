//! Prompt construction. The system prompt and tool schema are stable and come
//! first, so vLLM prefix caching hits on every step; the changing parts
//! (screenshot, element list, last 3 actions) come last.

use crate::element::Grounding;
use crate::schema::Decision;
use crate::skill::Skill;

/// The stable system prompt. Keep this byte-for-byte constant across a session
/// so the cached prefix is reused. Skill names/descriptions are appended, which
/// only change when the installed skill set changes (rare).
pub fn system_prompt(skills: &[Skill]) -> String {
    let mut s = String::from(SYSTEM_BASE);
    if !skills.is_empty() {
        s.push_str("\n\nAvailable skills (ask yourself if one fits before acting):\n");
        for sk in skills {
            s.push_str(&sk.prompt_line());
            s.push('\n');
        }
    }
    s
}

const SYSTEM_BASE: &str = "\
You are Kai, an agent that operates a computer for the user by looking at the \
screen and choosing ONE action at a time.

You are given a screenshot with numbered boxes drawn on interactive elements, \
and a text list of those elements like `#14 button \"Save\" (enabled)`. To act \
on an element, use its number as `target`. Never guess raw pixel coordinates. \
If the screen shows a grid instead of numbered boxes, refer to a cell like D7 \
via `target` as the cell index.

Rules:
- Reply with ONE JSON object for the next action, and nothing else.
- `thought` is at most 25 words.
- Prefer `open_app`/`open_url` to reach apps and sites directly.
- Set `risk` honestly: `high` for anything that sends, buys, deletes, submits a \
form, changes system settings, or types into a password field.
- Use `shell` with the command in `text` for terminal work (files, network \
diagnostics, builds). You will see its output in your last steps. Prefer it over \
clicking when the task is naturally a command.
- Use `ask_user` when you genuinely need the user to choose or provide input.
- Use `done` the moment the task is complete.
- `expect` describes what the screen should show after your action.";

/// Build the changing user turn: skill body (if one is active), the element
/// list, the last-3-step history, and the task. No old screenshots are ever
/// included here — only the current one goes in the image part.
pub fn user_turn(
    task: &str,
    grounding: &Grounding,
    history: &[HistoryItem],
    active_skill: Option<&Skill>,
    no_progress: bool,
) -> String {
    let mut s = String::new();

    if let Some(sk) = active_skill {
        s.push_str("Active skill: ");
        s.push_str(&sk.front.name);
        s.push('\n');
        s.push_str(&sk.body);
        s.push_str("\n\n");
    }

    s.push_str("Elements on screen:\n");
    s.push_str(&grounding.as_text());
    s.push_str("\n\n");

    if !history.is_empty() {
        s.push_str("Your last steps:\n");
        for h in history.iter().rev().take(3).rev() {
            s.push_str(&format!("- {}\n", h.summary));
        }
        s.push('\n');
    }

    if no_progress {
        s.push_str(
            "WARNING: the screen has not changed for two steps. Try a different \
             approach or ask the user.\n\n",
        );
    }

    s.push_str("Task: ");
    s.push_str(task);
    s.push_str("\n\nRespond with the next action as JSON.");
    s
}

/// A compact record of a past step for the history window.
#[derive(Debug, Clone)]
pub struct HistoryItem {
    pub summary: String,
}

impl HistoryItem {
    /// Attach command output (truncated) so the model sees what a shell step did.
    pub fn push_output(&mut self, out: &str) {
        const MAX: usize = 1500;
        let trimmed: String = out.chars().take(MAX).collect();
        let more = if out.chars().count() > MAX {
            " …(truncated)"
        } else {
            ""
        };
        self.summary
            .push_str(&format!("\n  output:\n{trimmed}{more}"));
    }

    pub fn from_decision(d: &Decision) -> Self {
        let mut summary = format!("{:?}", d.action).to_lowercase();
        if let Some(t) = d.target {
            summary.push_str(&format!(" #{t}"));
        }
        if let Some(text) = &d.text
            && !text.is_empty()
        {
            let short: String = text.chars().take(30).collect();
            summary.push_str(&format!(" \"{short}\""));
        }
        Self { summary }
    }
}

/// Pick the most relevant skill for a task by keyword overlap between the task
/// and each skill's name + description. Returns the best match if it clears a
/// small threshold. This is the cheap keyword path; the model can also pick.
pub fn select_skill<'a>(task: &str, skills: &'a [Skill]) -> Option<&'a Skill> {
    let task_words: Vec<String> = tokenize(task);
    if task_words.is_empty() {
        return None;
    }
    let mut best: Option<(&Skill, usize)> = None;
    for sk in skills {
        let hay = format!(
            "{} {}",
            sk.front.name.replace('-', " "),
            sk.front.description
        );
        let hay_words = tokenize(&hay);
        let score = task_words
            .iter()
            .filter(|w| w.len() > 2 && hay_words.contains(w))
            .count();
        if score > best.map(|(_, s)| s).unwrap_or(0) {
            best = Some((sk, score));
        }
    }
    best.filter(|(_, s)| *s >= 2).map(|(sk, _)| sk)
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Element, Rect, Role};
    use crate::skill::load_from_str;
    use std::path::Path;

    fn sample_skills() -> Vec<Skill> {
        let a = load_from_str(
            "---\nname: write-email-gmail\ndescription: Compose and send an email in Gmail. Use to write or send mail.\n---\nbody\n",
            Path::new("/tmp/write-email-gmail"),
        )
        .unwrap();
        let b = load_from_str(
            "---\nname: rename-files-in-folder\ndescription: Rename files in a folder. Use to batch rename files.\n---\nbody\n",
            Path::new("/tmp/rename-files-in-folder"),
        )
        .unwrap();
        vec![a, b]
    }

    #[test]
    fn system_prompt_is_stable_then_skills() {
        let p = system_prompt(&sample_skills());
        assert!(p.starts_with("You are Kai"));
        assert!(p.contains("write-email-gmail"));
    }

    #[test]
    fn user_turn_puts_task_last_and_elements_present() {
        let g = Grounding::Marks(vec![Element {
            mark: 1,
            role: Role::Button,
            name: "Save".into(),
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
            enabled: true,
            secure: false,
        }]);
        let t = user_turn("click save", &g, &[], None, false);
        assert!(t.contains("#1 button \"Save\""));
        assert!(t.trim_end().ends_with("JSON."));
        assert!(t.contains("Task: click save"));
    }

    #[test]
    fn selects_skill_by_keywords() {
        let skills = sample_skills();
        let picked = select_skill("please send an email to my professor", &skills).unwrap();
        assert_eq!(picked.front.name, "write-email-gmail");
    }

    #[test]
    fn no_skill_when_nothing_matches() {
        let skills = sample_skills();
        assert!(select_skill("play some jazz", &skills).is_none());
    }

    #[test]
    fn history_window_keeps_last_three() {
        let g = Grounding::Marks(vec![]);
        let hist: Vec<HistoryItem> = (0..5)
            .map(|i| HistoryItem {
                summary: format!("step{i}"),
            })
            .collect();
        let t = user_turn("x", &g, &hist, None, false);
        assert!(t.contains("step2") && t.contains("step4"));
        assert!(!t.contains("step0"));
    }
}
