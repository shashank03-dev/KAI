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

/// [`system_prompt`] plus the `plan` rule, for runs with the executor on.
/// Stable for the whole run, so the cached prefix still hits.
pub fn system_prompt_with_plan(skills: &[Skill]) -> String {
    let mut s = String::from(SYSTEM_BASE);
    s.push_str(PLAN_RULE);
    if !skills.is_empty() {
        s.push_str(&system_prompt(skills)[SYSTEM_BASE.len()..]);
    }
    s
}

/// The executor addition: short on purpose (it is in every request).
pub const PLAN_RULE: &str = "\n- When the steps after this one are predictable, add \
`plan`: up to 6 subgoals like {\"do\":\"type\",\"target\":\"Email field\",\
\"text\":\"a@b.c\",\"expect\":\"email typed\"} (`do`: click, type, key, select, \
open_url, open_app, shell or done; name elements in words, not numbers). An \
executor carries them out and calls you back on any doubt.";

const SYSTEM_BASE: &str = "\
You are Kai, an agent that operates a computer for the user by looking at the \
screen and choosing the next action.

You get a screenshot with numbered boxes on interactive elements, and a text \
list of them like `#14 button \"Save\"` (state shown only if disabled or \
secure). To act on an element, use its number as `target`. Never guess pixel \
coordinates. If the screen shows a numbered grid instead, use the cell number \
as `target`. Some steps have no screenshot, only the list.

Rules:
- Reply with ONE JSON object for the next action, and nothing else.
- `thought` is at most 25 words.
- Prefer `open_app`/`open_url` to reach apps and sites directly.
- Set `risk` honestly: `high` for anything that sends, buys, deletes, submits a \
form, changes system settings, or types into a password field.
- Use `shell` with the command in `text` for terminal work (files, network \
diagnostics, builds). You will see its output in your last steps. Prefer it over \
clicking when the task is naturally a command.
- Use `ask_user` with your question in `text` when you genuinely need the user \
to choose or provide input; their answer comes back and you carry on.
- Never try to solve a CAPTCHA or \"I'm not a robot\" check, and never guess a \
verification code: Kai pauses and the user does those.
- Use `done` the moment the task is complete. A command that ended with \
`[exit 0]` worked; never run the same command twice.
- Never open a page or app you already have open: reopening clears what you \
typed. For a dropdown (combobox), use `type` with the option's text.
- `expect` describes what the screen should show after your action.
- Only when the next actions are certain (e.g. type, then press Enter), list up \
to 3 of them in `then`, each like {\"action\":\"key\",\"keys\":[\"enter\"]}. \
They run in order without asking you; never put risky actions or `done` there.
- With no screenshot, if you must see the screen, reply `wait` with \
`need_screen` true.";

/// Everything one user turn is built from.
pub struct TurnInput<'a> {
    pub task: &'a str,
    pub grounding: &'a Grounding,
    pub history: &'a [HistoryItem],
    pub active_skill: Option<&'a Skill>,
    pub no_progress: bool,
    /// Extra lines for this step: steering messages, verifier warnings, the
    /// zoom note. Each goes on its own line before the task.
    pub notes: &'a [String],
    /// False on tree-only steps (no screenshot attached).
    pub has_screenshot: bool,
    /// Lessons from earlier runs on this computer (experience memory), as
    /// rendered by [`crate::memory::recall::render`]. Placed at the top of
    /// the turn, right after the stable system prompt and skill body: it only
    /// changes when the apps or sites in play change, so it rides in the
    /// cached prefix. Hints for the model only.
    pub lessons: Option<&'a str>,
    /// With `Config::macro_history`: the verified outcomes (redacted). The
    /// turn then shows a summary of older steps plus the last 2 raw ones
    /// ([`crate::macro_history`]); `None` keeps the last-3 window.
    pub macro_history: Option<&'a [String]>,
}

/// Build the changing user turn: skill body (if one is active), the compact
/// element list, the last-3-step history, and the task last. No old
/// screenshots are ever included — only the current one, if any.
pub fn user_turn(
    task: &str,
    grounding: &Grounding,
    history: &[HistoryItem],
    active_skill: Option<&Skill>,
    no_progress: bool,
) -> String {
    build_turn(&TurnInput {
        task,
        grounding,
        history,
        active_skill,
        no_progress,
        notes: &[],
        has_screenshot: true,
        lessons: None,
        macro_history: None,
    })
}

/// [`user_turn`] with notes and the screenshot flag.
pub fn build_turn(t: &TurnInput) -> String {
    let mut s = String::new();

    if let Some(sk) = t.active_skill {
        s.push_str("Active skill: ");
        s.push_str(&sk.front.name);
        s.push('\n');
        // `{skill_dir}` lets a skill point at its own helper scripts.
        let dir = std::fs::canonicalize(&sk.dir).unwrap_or_else(|_| sk.dir.clone());
        s.push_str(&sk.body.replace("{skill_dir}", &dir.display().to_string()));
        s.push_str("\n\n");
    }

    if let Some(l) = t.lessons.filter(|l| !l.trim().is_empty()) {
        s.push_str(l.trim_end());
        s.push_str("\n\n");
    }

    if !t.has_screenshot {
        s.push_str("(No screenshot this step.)\n");
    }
    s.push_str("Elements:\n");
    s.push_str(&t.grounding.as_compact_text());
    s.push_str("\n\n");

    let window = match t.macro_history {
        Some(_) => crate::macro_history::RAW_STEPS,
        None => 3,
    };
    let split = t.history.len().saturating_sub(window);
    if let Some(verified) = t.macro_history
        && let Some(line) = crate::macro_history::summary(&t.history[..split], verified)
    {
        s.push_str(&line);
        s.push_str("\n\n");
    }
    let recent: Vec<&HistoryItem> = t.history[split..].iter().collect();
    if !recent.is_empty() {
        s.push_str("Your last steps:\n");
        let last = recent.len() - 1;
        for (i, h) in recent.iter().enumerate() {
            s.push_str("- ");
            s.push_str(&h.summary);
            if let Some(out) = &h.output {
                // The newest output in full (capped); older ones shortened.
                let cap = if i == last {
                    OUTPUT_MAX
                } else {
                    OLD_OUTPUT_MAX
                };
                s.push_str("\n  output:\n");
                s.push_str(&truncate(out, cap));
            }
            s.push('\n');
        }
        s.push('\n');
    }

    if t.no_progress {
        s.push_str(
            "WARNING: the screen has not changed for two steps. Try a different \
             approach or ask the user.\n",
        );
    }
    for n in t.notes {
        s.push_str(n);
        s.push('\n');
    }
    if t.no_progress || !t.notes.is_empty() {
        s.push('\n');
    }

    s.push_str("Task: ");
    s.push_str(t.task);
    s.push_str("\n\nRespond with the next action as JSON.");
    s
}

const OUTPUT_MAX: usize = 1500;
const OLD_OUTPUT_MAX: usize = 300;

fn truncate(out: &str, max: usize) -> String {
    let trimmed: String = out.chars().take(max).collect();
    if out.chars().count() > max {
        format!("{trimmed} …(truncated)")
    } else {
        trimmed
    }
}

/// A compact record of a past step for the history window.
#[derive(Debug, Clone, Default)]
pub struct HistoryItem {
    pub summary: String,
    /// Command output of a `shell` step, shown under the summary.
    pub output: Option<String>,
    /// The name of the element the step acted on, for the macro summary
    /// (mark numbers change every frame).
    pub target_name: Option<String>,
}

impl HistoryItem {
    pub fn note(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            ..Default::default()
        }
    }

    /// [`Self::from_decision`] plus the target element's name.
    pub fn from_step(d: &Decision, grounding: &Grounding) -> Self {
        let mut h = Self::from_decision(d);
        h.target_name = d
            .target
            .and_then(|t| grounding.element(t))
            .map(|e| e.name.clone());
        h
    }

    /// Attach command output so the model sees what a shell step did.
    pub fn push_output(&mut self, out: &str) {
        // A silent success reads as "nothing happened" unless we say so.
        let out = if out.trim() == "[exit 0]" {
            "(no output) [exit 0 = success]"
        } else {
            out
        };
        self.output = Some(out.to_string());
    }

    pub fn from_decision(d: &Decision) -> Self {
        let mut summary = format!("{:?}", d.action).to_lowercase();
        if let Some(t) = d.target {
            summary.push_str(&format!(" #{t}"));
        }
        if let Some(text) = &d.text
            && !text.is_empty()
        {
            // Short commands are kept whole (a cut command looks like a wrong
            // one); a long one (a script) shows its start and says it ran in
            // full, or the model "finishes" it again. Typed text is shortened.
            if d.action == crate::schema::Action::Shell && text.chars().count() > 300 {
                let first: String = text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(160)
                    .collect();
                let lines = text.lines().count();
                summary.push_str(&format!(" \"{first}\" ({lines} lines, ran in full)"));
            } else {
                let cap = if d.action == crate::schema::Action::Shell {
                    300
                } else {
                    30
                };
                let short: String = text.chars().take(cap).collect();
                summary.push_str(&format!(" \"{short}\""));
            }
        }
        if !d.keys.is_empty() {
            summary.push(' ');
            summary.push_str(&d.keys.join("+"));
        }
        Self::note(summary)
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
            .filter(|w| w.len() > 2 && !STOPWORDS.contains(&w.as_str()) && hay_words.contains(w))
            .count();
        if score > best.map(|(_, s)| s).unwrap_or(0) {
            best = Some((sk, score));
        }
    }
    best.filter(|(_, s)| *s >= 2).map(|(sk, _)| sk)
}

/// Words too common to say which skill fits.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "use", "when", "user", "wants", "that", "this", "from", "into",
    "your", "you", "my", "can", "please", "then", "what", "which", "are", "was", "all", "any",
];

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
            value: None,
        }]);
        let t = user_turn("click save", &g, &[], None, false);
        assert!(t.contains("#1 button \"Save\"\n"));
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
    fn common_words_alone_do_not_pick_a_skill() {
        let skills = sample_skills();
        assert!(select_skill("use the thing and then the other for me", &skills).is_none());
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
            .map(|i| HistoryItem::note(format!("step{i}")))
            .collect();
        let t = user_turn("x", &g, &hist, None, false);
        assert!(t.contains("step2") && t.contains("step4"));
        assert!(!t.contains("step0"));
    }

    /// A typical browser screen: 40 elements, mostly enabled, a few unnamed.
    fn typical_screen() -> Grounding {
        let names = [
            "Back",
            "Forward",
            "Reload",
            "Home",
            "Address and search bar",
            "Bookmark this tab",
            "Extensions",
            "Downloads",
            "Open application menu",
            "New Tab",
            "Close Tab",
            "Wikipedia, the free encyclopedia",
            "Search Wikipedia",
            "Search",
            "Main page",
            "Contents",
            "Current events",
            "Random article",
            "About Wikipedia",
            "Contact us",
            "Help",
            "Learn to edit",
            "Community portal",
            "Recent changes",
            "Upload file",
            "Create account",
            "Log in",
            "Talk",
            "Read",
            "View source",
            "View history",
            "Tools",
            "",
            "",
            "Language",
            "Download as PDF",
            "Printable version",
            "Permanent link",
            "Page information",
            "Cite this page",
        ];
        Grounding::Marks(
            names
                .iter()
                .enumerate()
                .map(|(i, n)| Element {
                    mark: i as u32 + 1,
                    role: if i % 3 == 0 { Role::Link } else { Role::Button },
                    name: n.to_string(),
                    bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                    enabled: i != 7,
                    secure: false,
                    value: None,
                })
                .collect(),
        )
    }

    #[test]
    fn compact_turn_is_smaller_than_the_old_format() {
        let g = typical_screen();
        let hist: Vec<HistoryItem> = [
            "open_url \"https://wikipedia.org\"",
            "click #13",
            "type #13 \"Rust\"",
        ]
        .iter()
        .map(|s| HistoryItem::note(*s))
        .collect();
        let task = "Find the Wikipedia article about the Rust programming language";
        let new = user_turn(task, &g, &hist, None, false);
        // The pre-compaction layout, rebuilt for comparison.
        let mut old = format!("Elements on screen:\n{}\n\nYour last steps:\n", g.as_text());
        for h in &hist {
            old.push_str(&format!("- {}\n", h.summary));
        }
        old.push_str(&format!(
            "\nTask: {task}\n\nRespond with the next action as JSON."
        ));
        eprintln!(
            "MEASURED user turn, 40 elements: old {} chars (~{} tokens), new {} chars (~{} tokens)",
            old.len(),
            old.len() / 4,
            new.len(),
            new.len() / 4
        );
        assert!(new.len() * 10 < old.len() * 9, "at least 10% smaller");
    }
}
