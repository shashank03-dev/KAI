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
}

impl Context {
    pub fn new(task: &str, apps: &[String], domains: &[String]) -> Self {
        let t = scope::of_task(task, &[]);
        let mut c = Self {
            task_words: t.task_words,
            apps: t.apps,
            domains: t.domains,
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

    /// A key that changes when the apps or sites in play change.
    pub fn key(&self) -> String {
        let mut a = self.apps.clone();
        a.sort();
        let mut d = self.domains.clone();
        d.sort();
        format!("{}|{}", a.join(","), d.join(","))
    }
}

/// How well a lesson fits, or `None` when it does not apply.
pub fn score(l: &Lesson, cx: &Context, now: i64) -> Option<f32> {
    if l.retired() {
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
    let applies = app_hit
        || domain_hit
        || overlap >= 2
        || (overlap >= 1 && (l.scope.is_global() || frac >= 0.5));
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
