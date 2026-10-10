//! Deterministic lessons from one run's trace. No model call: the rules read
//! what the trace already records (errors, outputs, `verified`, steering
//! messages, the check).
//!
//! - **Recovery**: a step failed or changed nothing, and a later step with the
//!   same intent (same `expect`, same target name, or the same app) worked →
//!   `Prefer` what worked.
//! - **Dead end**: the same action on the same thing twice with no progress →
//!   `Avoid`; a shell command that errored or timed out → `Avoid`, with the
//!   corrected command attached when one later worked.
//! - **Steering**: each mid-run user message → a `Preference` (source User).
//! - **False done**: said done but the check failed → `Avoid` a bare `done`.

use super::scrub::{allowed, scrub, secure_typed};
use super::{Kind, Lesson, Scope, Source, clip_words, scope};
use crate::failure::{self, Category};
use crate::schema::Action;
use crate::trace::{Outcome, StepRecord, Trace};

/// At most this many derived lessons per run (steering messages aside): a
/// bad run should not flood memory.
pub const MAX_DERIVED: usize = 4;

/// Did this step fail outright (act error, error output, failing command)?
pub fn failed(s: &StepRecord) -> bool {
    let out = s.output.as_deref().unwrap_or("");
    s.path.as_deref() == Some("error")
        || out.starts_with("ERROR:")
        || (s.decision.action == Action::Shell && shell_failed(out))
}

/// A shell output that timed out or ended with a non-zero `[exit N]`.
pub fn shell_failed(out: &str) -> bool {
    if out.to_lowercase().contains("timed out") {
        return true;
    }
    let Some(i) = out.rfind("[exit ") else {
        return false;
    };
    let code = out[i + 6..].trim_end().trim_end_matches(']');
    code.parse::<i32>().is_ok_and(|c| c != 0)
}

/// The step acted but nothing changed (and nobody skipped it).
fn no_change(s: &StepRecord) -> bool {
    !s.verified
        && s.approval.as_deref() != Some("skipped")
        && !matches!(
            s.decision.action,
            Action::Wait | Action::Done | Action::AskUser | Action::Shell
        )
}

fn worked(s: &StepRecord) -> bool {
    s.verified
        && !failed(s)
        && !matches!(
            s.decision.action,
            Action::Wait | Action::Done | Action::AskUser
        )
}

fn target_name(s: &StepRecord) -> Option<String> {
    let t = s.decision.target?;
    failure::mark_name(&s.elements, t).filter(|n| !n.trim().is_empty())
}

fn short(s: &str, max: usize) -> String {
    let s = s.trim().replace('\n', " ");
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s
    }
}

/// The app a shell command starts, when its first word is an app name.
fn shell_app(s: &StepRecord) -> Option<String> {
    if s.decision.action != Action::Shell {
        return None;
    }
    let first = s.decision.text.as_deref()?.split_whitespace().next()?;
    let apps = scope::apps_in(first, &[]);
    apps.into_iter().next()
}

/// Token overlap of two `expect` texts, ignoring filler words.
fn expect_similarity(a: &str, b: &str) -> f32 {
    const FILLER: &[&str] = &[
        "the", "a", "an", "is", "are", "be", "now", "should", "shows", "show", "appears",
        "visible", "on", "screen", "and", "with", "of", "in",
    ];
    let toks = |s: &str| -> std::collections::BTreeSet<String> {
        super::norm_tokens(s)
            .into_iter()
            .filter(|w| !FILLER.contains(&w.as_str()))
            .collect()
    };
    let (ta, tb) = (toks(a), toks(b));
    let union = ta.union(&tb).count();
    if union == 0 {
        return 0.0;
    }
    ta.intersection(&tb).count() as f32 / union as f32
}

/// Same goal, different means?
fn same_intent(a: &StepRecord, b: &StepRecord) -> bool {
    let (ea, eb) = (a.decision.expect.trim(), b.decision.expect.trim());
    if !ea.is_empty() && !eb.is_empty() && expect_similarity(ea, eb) >= 0.6 {
        return true;
    }
    if let (Some(x), Some(y)) = (target_name(a), target_name(b))
        && x.eq_ignore_ascii_case(&y)
    {
        return true;
    }
    match (shell_app(a), b.decision.action) {
        (Some(app), Action::OpenApp) => {
            scope::app_name(b.decision.text.as_deref().unwrap_or("")).as_deref() == Some(&app)
        }
        _ => false,
    }
}

/// A step in words, for a lesson: `open_app 'Mousepad'`, ``shell `ls` ``,
/// `click 'Save'`, `key ctrl+s`.
pub fn what(s: &StepRecord) -> String {
    let d = &s.decision;
    let act = format!("{:?}", d.action);
    let act = snake(&act);
    match d.action {
        Action::Shell => format!("shell `{}`", short(d.text.as_deref().unwrap_or(""), 60)),
        Action::OpenApp | Action::OpenUrl => {
            format!("{act} '{}'", short(d.text.as_deref().unwrap_or(""), 50))
        }
        Action::Key => format!("key {}", d.keys.join("+")),
        _ => match target_name(s) {
            Some(n) => format!("{act} on '{}'", short(&n, 40)),
            None => act,
        },
    }
}

fn snake(camel: &str) -> String {
    let mut out = String::new();
    for (i, c) in camel.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn in_app(app: &Option<String>) -> String {
    app.as_ref().map(|a| format!(" in {a}")).unwrap_or_default()
}

/// The last line of a failing command's output, without the exit marker.
fn short_error(out: &str) -> String {
    if out.to_lowercase().contains("timed out") {
        return "it timed out (a GUI app or a prompt blocks a shell step)".into();
    }
    let line = out
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("[exit"))
        .unwrap_or("non-zero exit");
    short(line, 60)
}

/// Lessons from one finished run. `check`: the task's automatic check, when
/// there is one (bench); `None` judges by the outcome.
pub fn learn(trace: &Trace, check: Option<bool>) -> Vec<Lesson> {
    let run = trace.started.as_str();
    let passed = failure::passed(trace, check);
    let run_scope = scope::of_run(trace);
    let secure = secure_typed(trace);
    let mut out: Vec<Lesson> = Vec::new();
    let steps = &trace.steps;

    let scoped = |app: &Option<String>| -> Scope {
        let mut s = Scope {
            task_words: run_scope.task_words.clone(),
            ..Default::default()
        };
        match app {
            Some(a) if scope::domain_of(a).as_deref() == Some(a.as_str()) => {
                s.domains.push(a.clone())
            }
            Some(a) => s.apps.push(a.clone()),
            None => {
                s.apps = run_scope.apps.clone();
                s.domains = run_scope.domains.clone();
            }
        }
        s
    };
    let typed_secure = |s: &StepRecord| {
        s.decision.action == Action::Type
            && s.decision
                .target
                .is_some_and(|t| crate::risk::mark_is_secure_in_text(&s.elements, t))
    };
    let push = |out: &mut Vec<Lesson>, kind, text: String, app: &Option<String>, tg| {
        let text = clip_words(&scrub(&text), super::MAX_WORDS);
        if !allowed(&text, &secure, false) || out.iter().any(|l: &Lesson| l.text == text) {
            return;
        }
        out.push(
            Lesson::new(kind, &text, scoped(app), Source::Derived)
                .with_evidence(run)
                .targeting(tg),
        );
    };

    // ---- Recovery and failing shell commands ----
    let mut fixed = vec![false; steps.len()];
    for (i, s) in steps.iter().enumerate() {
        if typed_secure(s) || !(failed(s) || no_change(s)) {
            continue;
        }
        let app = scope::at_step(trace, i);
        let fix = steps.iter().enumerate().skip(i + 1).find(|(_, b)| {
            worked(b) && !typed_secure(b) && same_intent(s, b) && what(b) != what(s)
        });
        if let Some((j, b)) = fix {
            fixed[i] = true;
            let text = if shell_app(s).is_some() && b.decision.action == Action::OpenApp {
                let name = b.decision.text.as_deref().unwrap_or("").trim();
                format!(
                    "Start {name} with open_app, not {}: a shell step waits for the app and blocks.",
                    what(s)
                )
            } else if s.decision.action == Action::Shell && b.decision.action == Action::Shell {
                format!(
                    "{} fails here: {}; {} worked instead.",
                    what(s),
                    short_error(s.output.as_deref().unwrap_or("")),
                    what(b)
                )
            } else {
                let goal = clip_words(b.decision.expect.trim(), 8);
                let goal = if goal.is_empty() {
                    "get this done".into()
                } else {
                    goal
                };
                format!(
                    "To reach \"{goal}\"{}, {} (not {}).",
                    in_app(&app),
                    what(b),
                    what(s)
                )
            };
            let kind = if s.decision.action == Action::Shell && b.decision.action == Action::Shell {
                Kind::Avoid
            } else {
                Kind::Prefer
            };
            let tg = if failed(s) {
                Category::EnvError
            } else {
                Category::Loop
            };
            let where_ = match b.decision.action {
                Action::OpenApp => scope::app_name(b.decision.text.as_deref().unwrap_or("")),
                _ => scope::at_step(trace, j),
            };
            push(&mut out, kind, text, &where_, Some(tg));
        }
    }
    for (i, s) in steps.iter().enumerate() {
        if fixed[i] || s.decision.action != Action::Shell || !failed(s) {
            continue;
        }
        let out_text = s.output.as_deref().unwrap_or("");
        // A failing command in a run that still passed is often fine (grep
        // with no match): only keep it when the run failed.
        if passed {
            continue;
        }
        let app = scope::at_step(trace, i);
        let text = match shell_app(s) {
            Some(a) if out_text.to_lowercase().contains("timed out") => format!(
                "{} blocks here (timed out): start {a} with open_app instead of shell.",
                what(s)
            ),
            _ => format!("{} fails here: {}.", what(s), short_error(out_text)),
        };
        push(&mut out, Kind::Avoid, text, &app, Some(Category::EnvError));
    }

    // ---- Dead ends: the same thing twice with no progress ----
    let mut seen: Vec<(String, usize, usize)> = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        if s.decision.action == Action::Shell || typed_secure(s) || !no_change(s) || fixed[i] {
            continue;
        }
        let key = what(s);
        match seen.iter_mut().find(|(k, _, _)| *k == key) {
            Some(e) => e.1 += 1,
            None => seen.push((key, 1, i)),
        }
    }
    for (key, n, i) in seen {
        if n >= 2 {
            let app = scope::at_step(trace, i);
            let text = format!(
                "{key}{} did nothing ({n} tries); try another way.",
                in_app(&app)
            );
            push(&mut out, Kind::Avoid, text, &app, Some(Category::Loop));
        }
    }

    // ---- False done ----
    if trace.outcome == Some(Outcome::Success) && check == Some(false) {
        let expect = steps
            .iter()
            .rev()
            .map(|s| s.decision.expect.trim())
            .find(|e| !e.is_empty())
            .map(|e| clip_words(e, 10))
            .unwrap_or_else(|| "the result on screen".into());
        let kind = clip_words(&run_scope.task_words.join(" "), 6);
        let text = format!("Before done on tasks like '{kind}', verify {expect}.");
        push(
            &mut out,
            Kind::Avoid,
            text,
            &None,
            Some(Category::FalseDone),
        );
    }

    out.truncate(MAX_DERIVED);

    // ---- Steering: the user's words, kept as preferences ----
    for m in &trace.steered {
        let text = clip_words(&scrub(m.trim()), super::MAX_WORDS - 3);
        if text.is_empty() || !allowed(&text, &secure, true) {
            continue;
        }
        let l = Lesson::new(
            Kind::Preference,
            &format!("User prefers: {text}"),
            scoped(&None),
            Source::User,
        )
        .with_evidence(run);
        out.push(l);
    }
    out
}

/// Did this run fail somewhere and still finish, or fail? Reflection only
/// runs for these.
pub fn worth_reflecting(trace: &Trace, check: Option<bool>) -> bool {
    !failure::passed(trace, check) || trace.steps.iter().any(|s| failed(s) || no_change(s))
}
