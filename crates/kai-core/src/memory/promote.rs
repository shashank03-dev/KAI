//! Promotion: when an app or site has both a remembered successful opening
//! (a plan-cache entry) and at least two `Prefer` lessons that helped, Kai
//! suggests turning them into a skill draft. The draft goes through the
//! normal Save-as-skill editor; nothing is ever installed automatically.

use super::{Kind, Lesson, scope};
use crate::plan_cache::CachedPlan;
use crate::schema::Action;

/// Helpful `Prefer` lessons a scope needs before a skill is suggested.
pub const MIN_HELPFUL: usize = 2;

/// A suggested skill.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// The app or site it is about.
    pub scope: String,
    /// The task of the remembered plan.
    pub task: String,
    /// Ids of the lessons it carries.
    pub lessons: Vec<String>,
    /// A spec-valid `SKILL.md` to open in the editor.
    pub draft: String,
}

/// Suggestions from the lessons and the plan cache's plans.
pub fn suggestions(lessons: &[Lesson], plans: &[CachedPlan]) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();
    for p in plans {
        let mut keys = scope::of_task(&p.task, &[]).apps;
        keys.extend(scope::domains_in(&p.task));
        for d in &p.steps {
            let t = d.text.as_deref().unwrap_or("");
            match d.action {
                Action::OpenApp => keys.extend(scope::app_name(t)),
                Action::OpenUrl => keys.extend(scope::domain_of(t)),
                _ => {}
            }
        }
        keys.dedup();
        for k in keys {
            if out.iter().any(|s| s.scope == k) {
                continue;
            }
            let good: Vec<&Lesson> = lessons
                .iter()
                .filter(|l| l.kind == Kind::Prefer && l.helped >= 1 && l.helped > l.hurt)
                .filter(|l| l.scope.apps.contains(&k) || l.scope.domains.contains(&k))
                .collect();
            if good.len() >= MIN_HELPFUL {
                out.push(Suggestion {
                    scope: k.clone(),
                    task: p.task.clone(),
                    lessons: good.iter().map(|l| l.id.clone()).collect(),
                    draft: skill_draft(&p.task, p, &good),
                });
            }
        }
    }
    out
}

/// A `SKILL.md`: the plan's opening as deterministic steps, the lessons as
/// instructions.
pub fn skill_draft(task: &str, plan: &CachedPlan, lessons: &[&Lesson]) -> String {
    let mut steps = Vec::new();
    for d in &plan.steps {
        let t = d.text.clone().unwrap_or_default();
        let step = match d.action {
            Action::OpenUrl => serde_json::json!({"do": "open_url", "url": t}),
            Action::OpenApp => serde_json::json!({"do": "open_app", "app": t}),
            Action::Key => serde_json::json!({"do": "shortcut", "keys": d.keys}),
            Action::Type if d.target.is_none() => serde_json::json!({"do": "type", "text": t}),
            _ => break,
        };
        steps.push(step);
    }
    let meta = if steps.is_empty() {
        String::new()
    } else {
        // Single-quoted YAML: double any single quote inside.
        let json = serde_json::Value::Array(steps)
            .to_string()
            .replace('\'', "''");
        format!("  kai: '{json}'\n")
    };
    let task = task.trim().trim_end_matches('.');
    let desc = task.replace(['\n', ':'], " ");
    let body: Vec<String> = lessons.iter().map(|l| format!("- {}", l.text)).collect();
    format!(
        "---\nname: {name}\ndescription: \"{desc}. Use when the user asks to {lower}.\"\nlicense: MIT\nmetadata:\n  author: you\n{meta}---\n# {task}\n\nLearned on this computer:\n\n{body}\n\nCheck the screen after each step and reply `done` when the task is finished.\n",
        name = slug(task),
        desc = desc.replace('"', "'"),
        lower = desc.to_lowercase().replace('"', "'"),
        body = body.join("\n"),
    )
}

/// A spec-valid skill name from a task: lowercase words joined by hyphens.
pub fn slug(task: &str) -> String {
    let words: Vec<String> = task
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(6)
        .map(String::from)
        .collect();
    let mut s = words.join("-");
    s.truncate(64);
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "my-skill".into() } else { s }
}
