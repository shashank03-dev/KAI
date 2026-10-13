//! Picking the few lessons worth showing: scope match + task-word overlap +
//! confidence (with age decay), top [`MAX_LESSONS`], at most
//! [`MAX_WORDS_TOTAL`] words in all. The model sees them as a short block in
//! its user turn; nothing else reads them.

use super::{Lesson, scope};

/// Lessons shown at most.
pub const MAX_LESSONS: usize = 5;
/// Words of lesson text shown at most, all lessons together.
pub const MAX_WORDS_TOTAL: usize = 120;
/// The block's first line.
pub const HEADER: &str = "From earlier runs on this computer:";

/// What the run is about right now: the task's words and the apps and sites
/// in play (named in the task, opened this run, or visible on screen).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    pub task_words: Vec<String>,
    pub apps: Vec<String>,
    pub domains: Vec<String>,
    /// The current screen's signature ([`scope::screen_sig`]), if known.
    pub screen: Option<String>,
}

impl Context {
    pub fn new(task: &str, apps: &[String], domains: &[String]) -> Self {
        let t = scope::of_task(task, &[]);
        let mut c = Self {
            task_words: t.task_words,
            apps: t.apps,
            domains: t.domains,
            screen: None,
        };
        for a in apps {
            if !c.apps.contains(a) {
                c.apps.push(a.clone());
            }
        }
        for d in domains {
            if !c.domains.contains(d) {
                c.domains.push(d.clone());
            }
        }
        c
    }

    /// The same context on a known screen.
    pub fn at_screen(mut self, screen: Option<String>) -> Self {
        self.screen = screen;
        self
    }

    /// A key that changes when the apps or sites in play, or the screen,
    /// change: recall runs again at each new decision point.
    pub fn key(&self) -> String {
        let mut a = self.apps.clone();
        a.sort();
        let mut d = self.domains.clone();
        d.sort();
        let screen = self.screen.as_deref().unwrap_or("");
        format!("{}|{}|{screen}", a.join(","), d.join(","))
    }
}

/// How well a lesson fits, or `None` when it does not apply.
pub fn score(l: &Lesson, cx: &Context, now: i64) -> Option<f32> {
    if l.retired() || l.pending {
        return None;
    }
    let app_hit = l.scope.apps.iter().any(|a| cx.apps.contains(a));
    let domain_hit = l.scope.domains.iter().any(|d| cx.domains.contains(d));
    let overlap = l
        .scope
        .task_words
        .iter()
        .filter(|w| cx.task_words.contains(w))
        .count();
    let denom = l.scope.task_words.len().min(cx.task_words.len()).max(1);
    let frac = overlap as f32 / denom as f32;
    // Measured (2026-10-10 live test): matching on shared task words alone
    // leaked lessons into unrelated tasks (bench prompts share words like
    // "folder" or "home"). A lesson tied to an app or site applies only when
    // that app or site is in play; one with neither needs a strong match.
    let scoped = !l.scope.apps.is_empty() || !l.scope.domains.is_empty();
    let applies = if scoped {
        app_hit || domain_hit
    } else {
        overlap >= 2 && frac >= 0.6
    };
    if !applies {
        return None;
    }
    let mut s = 1.5 * frac + l.weight(now);
    if app_hit {
        s += 2.0;
    }
    if domain_hit {
        s += 2.0;
    }
    // Learned on a particular screen: best there, weaker elsewhere.
    if !l.scope.screens.is_empty()
        && let Some(here) = &cx.screen
    {
        s += if l.scope.screens.contains(here) {
            2.0
        } else {
            -1.0
        };
    }
    Some(s)
}

/// The best lessons for `cx`, best first, within the count and word budget.
pub fn rank<'a>(lessons: &'a [Lesson], cx: &Context, now: i64) -> Vec<&'a Lesson> {
    let mut scored: Vec<(&Lesson, f32)> = lessons
        .iter()
        .filter_map(|l| score(l, cx, now).map(|s| (l, s)))
        .collect();
    scored.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| b.0.touched().cmp(&a.0.touched()))
    });
    let mut out = Vec::new();
    let mut words = 0;
    for (l, _) in scored {
        if out.len() >= MAX_LESSONS {
            break;
        }
        let w = l.words();
        if words + w > MAX_WORDS_TOTAL {
            continue;
        }
        words += w;
        out.push(l);
    }
    out
}

/// The block for the user turn, or `None` when there is nothing to say.
pub fn render(lessons: &[Lesson]) -> Option<String> {
    if lessons.is_empty() {
        return None;
    }
    let mut s = String::from(HEADER);
    for l in lessons {
        s.push_str("\n- ");
        s.push_str(&l.text);
    }
    Some(s)
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use crate::memory::{Kind, Lesson, Scope, Source};

    fn lesson(apps: &[&str], words: &[&str]) -> Lesson {
        let scope = Scope {
            apps: apps.iter().map(|s| s.to_string()).collect(),
            domains: vec![],
            task_words: words.iter().map(|s| s.to_string()).collect(),
            screens: Vec::new(),
        };
        Lesson::new(
            Kind::Prefer,
            "Start Mousepad with open_app, not shell.",
            scope,
            Source::Derived,
        )
    }

    #[test]
    fn app_lessons_stay_with_their_app() {
        let l = lesson(&["mousepad"], &["kai-bench", "folder", "line", "numbers"]);
        let other = Context::new(
            "Zip the folder kai-bench/photos in my home folder",
            &[],
            &[],
        );
        assert!(
            score(&l, &other, 0).is_none(),
            "leaked into an unrelated task"
        );
        let same = Context::new("Turn on line numbers", &["mousepad".into()], &[]);
        assert!(score(&l, &same, 0).is_some());
    }

    #[test]
    fn the_screen_a_lesson_was_learned_on_ranks_it_higher() {
        let mut here = lesson(&["mousepad"], &[]);
        here.scope.screens = vec!["mousepad/button,textfield".into()];
        let anywhere = lesson(&["mousepad"], &[]);
        let cx = |screen: &str| {
            Context::new("write a note", &["mousepad".into()], &[]).at_screen(Some(screen.into()))
        };
        let on = cx("mousepad/button,textfield");
        let off = cx("mousepad/button,menuitem");
        assert!(score(&here, &on, 0) > score(&anywhere, &on, 0));
        assert!(score(&here, &off, 0) < score(&anywhere, &off, 0));
        // Still shown elsewhere in the app, just ranked lower.
        assert!(score(&here, &off, 0).is_some());
        // The screen is part of the recall key: a new screen recalls again.
        assert_ne!(on.key(), off.key());
    }

    #[test]
    fn screen_signatures_ignore_the_document_name() {
        use crate::element::{Element, Grounding, Rect, Role};
        let g = |roles: &[Role]| {
            Grounding::Marks(
                roles
                    .iter()
                    .enumerate()
                    .map(|(i, r)| Element {
                        mark: i as u32 + 1,
                        role: *r,
                        name: format!("e{i}"),
                        bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                        enabled: true,
                        secure: false,
                        focused: false,
                        value: None,
                    })
                    .collect(),
            )
        };
        let editor = g(&[Role::Button, Role::TextField, Role::Button]);
        let a = scope::screen_sig(Some("notes.txt - Mousepad"), &editor);
        let b = scope::screen_sig(Some("todo.md - Mousepad"), &editor);
        assert_eq!(a.as_deref(), Some("mousepad/button,textfield"));
        assert_eq!(a, b);
        // A dialog adds kinds of elements: a different screen.
        let dialog = g(&[Role::Button, Role::TextField, Role::ComboBox]);
        assert_ne!(a, scope::screen_sig(Some("Save As - Mousepad"), &dialog));
        assert_eq!(scope::screen_sig(None, &editor), None);
    }
}
