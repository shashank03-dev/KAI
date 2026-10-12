//! `kai memory`: view and edit what Kai learned on this computer
//! (`<config>/memory/lessons.jsonl`, docs/MEMORY.md).

use clap::Subcommand;
use kai_core::memory::{Kind, Lesson, MemoryStore, Scope, promote};
use kai_core::settings::Settings;
use std::process::ExitCode;

#[derive(Subcommand)]
pub enum MemoryCmd {
    /// List lessons, most confident first.
    List {
        /// Only lessons for this app (e.g. mousepad) or site (github.com).
        #[arg(long)]
        app: Option<String>,
    },
    /// Delete one lesson by id (or a unique start of it).
    Forget { id: String },
    /// Delete every lesson.
    Clear,
    /// Teach Kai something yourself (at most 30 words).
    Add {
        text: String,
        /// The app it is about (repeatable).
        #[arg(long)]
        app: Vec<String>,
        /// The site it is about (repeatable).
        #[arg(long)]
        site: Vec<String>,
        /// avoid, prefer, fact or preference.
        #[arg(long, default_value = "preference")]
        kind: String,
    },
    /// Skill drafts suggested by proven lessons (never installed for you).
    Suggest,
    /// List remembered procedures (docs/PROCEDURES.md), or forget one by
    /// its number in the list.
    Procedures {
        #[arg(long)]
        forget: Option<usize>,
    },
}

fn store() -> anyhow::Result<MemoryStore> {
    Ok(MemoryStore::load(Settings::memory_dir())?)
}

/// One `kai memory list` line.
pub fn row(l: &Lesson) -> String {
    let mut scope: Vec<String> = l.scope.apps.clone();
    scope.extend(l.scope.domains.iter().cloned());
    let scope = if scope.is_empty() {
        "any app".to_string()
    } else {
        scope.join(", ")
    };
    format!(
        "{}  {:<10} {:>3.0}%  used {:<3} {:<22} {}",
        l.id,
        l.kind.as_str(),
        l.confidence() * 100.0,
        l.uses,
        scope,
        l.text
    )
}

pub fn run(cmd: MemoryCmd) -> anyhow::Result<ExitCode> {
    let s = store()?;
    match cmd {
        MemoryCmd::List { app } => {
            let want = app.map(|a| a.to_lowercase());
            let lessons: Vec<Lesson> = s
                .lessons()
                .into_iter()
                .filter(|l| {
                    want.as_ref()
                        .is_none_or(|w| l.scope.apps.contains(w) || l.scope.domains.contains(w))
                })
                .collect();
            if lessons.is_empty() {
                println!(
                    "No lessons yet. Kai learns from runs that go wrong; `kai memory add` teaches it directly."
                );
            }
            for l in &lessons {
                println!("{}", row(l));
            }
            if let Some(p) = s.path() {
                eprintln!("{} lessons in {}", lessons.len(), p.display());
            }
        }
        MemoryCmd::Forget { id } => {
            anyhow::ensure!(s.forget(&id)?, "no single lesson with id {id}");
            println!("Forgot {id}.");
        }
        MemoryCmd::Clear => {
            let n = s.len();
            s.clear()?;
            println!("Deleted {n} lessons.");
        }
        MemoryCmd::Add {
            text,
            app,
            site,
            kind,
        } => {
            let kind = Kind::parse(&kind)
                .ok_or_else(|| anyhow::anyhow!("--kind is avoid, prefer, fact or preference"))?;
            let scope = Scope {
                apps: app.iter().map(|a| a.to_lowercase()).collect(),
                domains: site
                    .iter()
                    .filter_map(|d| kai_core::memory::scope::domain_of(d))
                    .collect(),
                task_words: kai_core::memory::scope::task_words(&text),
            };
            let l = s.add_user(kind, &text, scope)?;
            println!("{}", row(&l));
        }
        MemoryCmd::Procedures { forget } => {
            let p = kai_core::ProcedureStore::load(Settings::dir().join("procedures"))?;
            let list = p.list();
            if let Some(n) = forget {
                let one = list
                    .get(n.wrapping_sub(1))
                    .ok_or_else(|| anyhow::anyhow!("no procedure {n}"))?;
                p.forget(&one.shape)?;
                println!("Forgot \"{}\".", one.task);
            } else if list.is_empty() {
                println!("No procedures yet (they are recorded with --procedures).");
            }
            for (i, pr) in list.iter().enumerate().filter(|_| forget.is_none()) {
                println!(
                    "{:>2}  {:<11} {} steps  ok {} / failed {}  {}",
                    i + 1,
                    format!("{:?}", pr.status).to_lowercase(),
                    pr.steps.len(),
                    pr.successes,
                    pr.failures,
                    pr.task
                );
            }
        }
        MemoryCmd::Suggest => {
            let plans = kai_core::PlanCache::load(Settings::dir().join("plans"))?.plans();
            let found = promote::suggestions(&s.lessons(), &plans);
            if found.is_empty() {
                println!(
                    "No suggestions yet: a skill is suggested once an app has a remembered \
                     opening and {} lessons that helped.",
                    promote::MIN_HELPFUL
                );
            }
            for sg in found {
                println!("# {} (from \"{}\")\n{}\n", sg.scope, sg.task, sg.draft);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kai_core::memory::Source;

    #[test]
    fn rows_show_kind_confidence_and_scope() {
        let mut l = Lesson::new(
            Kind::Avoid,
            "Do not start Mousepad from the shell.",
            Scope {
                apps: vec!["mousepad".into()],
                ..Default::default()
            },
            Source::Derived,
        );
        l.helped = 2;
        let r = row(&l);
        assert!(r.contains("avoid"));
        assert!(r.contains(" 75%"));
        assert!(r.contains("mousepad"));
        assert!(r.ends_with("Do not start Mousepad from the shell."));
    }
}
