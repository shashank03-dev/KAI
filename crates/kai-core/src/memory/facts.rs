//! App facts: how to reach something that is not on screen
//! (`docs/MEMORY.md`, "App facts"). The failure the memory papers single
//! out is hidden-operation blindness: the command the task needs sits in a
//! menu or behind a shortcut. When a run that passed reached its goal
//! through a menu path or a shortcut, Kai keeps that as a `fact` scoped to
//! the app, not to the task's words, so any later task in that app sees it.
//! No model call: the rules read the trace.

use super::scrub::{allowed, scrub};
use super::{Kind, Lesson, MAX_WORDS, Scope, Source, clip_words, scope};
use crate::element::Role;
use crate::schema::Action;
use crate::trace::{StepRecord, Trace};

/// Facts per run at most.
pub const MAX_FACTS: usize = 2;

/// Shortcuts everyone knows: not worth a fact.
const COMMON_KEYS: &[&str] = &[
    "ctrl+c", "ctrl+v", "ctrl+x", "ctrl+a", "ctrl+z", "ctrl+y", "alt+tab", "ctrl+l", "ctrl+t",
    "ctrl+w", "ctrl+f", "ctrl+s", "ctrl+o", "ctrl+n", "ctrl+p", "ctrl+q", "ctrl+r",
];

/// Facts from a run that passed. `secure`: text typed into secure fields,
/// which never ends up in a lesson.
pub fn learn(trace: &Trace, secure: &[String]) -> Vec<Lesson> {
    let steps = &trace.steps;
    let mut out: Vec<Lesson> = Vec::new();
    let mut push = |app: Option<String>, goal: &str, how: String| {
        let Some(app) = app else { return };
        let goal = clip_words(goal.trim().trim_end_matches('.'), 8);
        if goal.is_empty() || out.len() >= MAX_FACTS {
            return;
        }
        let text = clip_words(&scrub(&format!("In {app}, {goal}: {how}.")), MAX_WORDS);
        if !allowed(&text, secure, false) || out.iter().any(|l| l.text == text) {
            return;
        }
        let scope = Scope {
            apps: vec![app],
            ..Default::default()
        };
        out.push(Lesson::new(Kind::Fact, &text, scope, Source::Derived));
    };

    let mut i = 0;
    while i < steps.len() {
        // A menu path: consecutive clicks on menu items that all worked.
        let run: Vec<&StepRecord> = steps[i..]
            .iter()
            .take_while(|s| is_menu_click(s) && s.verified)
            .collect();
        if run.len() >= 2 {
            let path: Vec<&str> = run
                .iter()
                .filter_map(|s| s.target.as_ref().map(|t| t.name.as_str()))
                .collect();
            let last = run[run.len() - 1];
            push(
                scope::at_step(trace, i),
                &last.decision.expect,
                format!("use the menu {}", path.join(" › ")),
            );
            i += run.len();
            continue;
        }
        let s = &steps[i];
        if let Some(keys) = shortcut(s) {
            push(
                scope::at_step(trace, i),
                &s.decision.expect,
                format!("press {keys}"),
            );
        }
        i += 1;
    }
    out
}

fn is_menu_click(s: &StepRecord) -> bool {
    s.decision.action == Action::Click
        && s.path.as_deref() != Some("human")
        && s.target.as_ref().is_some_and(|t| t.role == Role::MenuItem)
}

/// A key combination with a modifier that worked and is not a common one.
fn shortcut(s: &StepRecord) -> Option<String> {
    if s.decision.action != Action::Key || !s.verified || s.path.as_deref() == Some("human") {
        return None;
    }
    let keys: Vec<String> = s.decision.keys.iter().map(|k| k.to_lowercase()).collect();
    let modded = keys.len() >= 2
        && keys
            .iter()
            .any(|k| matches!(k.as_str(), "ctrl" | "control" | "alt" | "super" | "meta"));
    let combo = keys.join("+");
    (modded && !COMMON_KEYS.contains(&combo.as_str())).then_some(combo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Decision;
    use crate::trace::{Outcome, TargetSig, Timings};

    fn step(raw: &str, target: Option<(Role, &str)>) -> StepRecord {
        let d = Decision::parse_checked(raw).unwrap();
        let mut s = StepRecord::new(0, &d, Timings::default(), "Mock");
        s.target = target.map(|(role, name)| TargetSig {
            role,
            name: name.into(),
        });
        s
    }

    fn run(steps: Vec<StepRecord>) -> Trace {
        let mut t = Trace::new("export the report");
        for s in steps {
            t.push(s);
        }
        t.finish(Outcome::Success);
        t
    }

    fn open_writer() -> StepRecord {
        step(
            r#"{"thought":"","action":"open_app","text":"libreoffice","risk":"none"}"#,
            None,
        )
    }

    #[test]
    fn a_menu_path_becomes_an_app_fact() {
        let t = run(vec![
            open_writer(),
            step(
                r#"{"thought":"","action":"click","target":3,"risk":"none","expect":"file menu"}"#,
                Some((Role::MenuItem, "File")),
            ),
            step(
                r#"{"thought":"","action":"click","target":9,"risk":"none","expect":"export as pdf dialog open"}"#,
                Some((Role::MenuItem, "Export as PDF…")),
            ),
        ]);
        let f = learn(&t, &[]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, Kind::Fact);
        assert!(f[0].text.contains("File › Export as PDF"), "{}", f[0].text);
        // Scoped to the app, not to this task's words.
        assert!(!f[0].scope.apps.is_empty() && f[0].scope.task_words.is_empty());
    }

    #[test]
    fn a_rare_shortcut_becomes_a_fact_and_common_ones_do_not() {
        let t = run(vec![
            open_writer(),
            step(
                r#"{"thought":"","action":"key","keys":["ctrl","shift","e"],"risk":"none","expect":"export dialog open"}"#,
                None,
            ),
            step(
                r#"{"thought":"","action":"key","keys":["ctrl","c"],"risk":"none","expect":"copied"}"#,
                None,
            ),
        ]);
        let f = learn(&t, &[]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].text.contains("press ctrl+shift+e"), "{}", f[0].text);
    }

    #[test]
    fn steps_that_changed_nothing_teach_nothing() {
        let mut k = step(
            r#"{"thought":"","action":"key","keys":["ctrl","shift","e"],"risk":"none","expect":"export dialog"}"#,
            None,
        );
        k.verified = false;
        assert!(learn(&run(vec![open_writer(), k]), &[]).is_empty());
    }

    #[test]
    fn no_goal_no_fact() {
        let k = step(
            r#"{"thought":"","action":"key","keys":["ctrl","shift","e"],"risk":"none"}"#,
            None,
        );
        assert!(learn(&run(vec![open_writer(), k]), &[]).is_empty());
    }
}
