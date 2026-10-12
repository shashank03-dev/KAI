//! `kai memory`: view and edit what Kai learned on this computer
//! (`<config>/memory/lessons.jsonl`, docs/MEMORY.md).

use clap::Subcommand;
use kai_core::memory::{Kind, Lesson, MemoryStore, Scope, consolidate, exam, promote};
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
    /// Read saved runs and propose lessons that recur across them (stored
    /// as pending: never shown to the model until accepted).
    Consolidate,
    /// List pending lessons.
    Pending,
    /// Make a pending lesson live, by id, or `all`.
    Accept { id: String },
    /// Drop a pending lesson, by id.
    Reject { id: String },
    /// Compare two bench results of the same tasks, one run with
    /// `--lessons current` and one with `--lessons candidate`; with
    /// `--accept`, make the pending lessons live if the candidate passes.
    Exam {
        current: std::path::PathBuf,
        candidate: std::path::PathBuf,
        /// Wins minus losses the candidate needs.
        #[arg(long, default_value_t = kai_core::memory::exam::MARGIN)]
        margin: i64,
        #[arg(long)]
        accept: bool,
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
                screens: Vec::new(),
            };
            let l = s.add_user(kind, &text, scope)?;
            println!("{}", row(&l));
        }
        MemoryCmd::Consolidate => {
            let traces = saved_traces(&Settings::traces_dir());
            let found = consolidate::propose(&traces, &s.lessons());
            if found.is_empty() {
                println!(
                    "Read {} runs: nothing recurs in {}+ runs that memory does not know yet.",
                    traces.len(),
                    consolidate::MIN_SUPPORT
                );
            }
            for l in s.propose(found)? {
                println!("pending  {}", row(&l));
            }
        }
        MemoryCmd::Pending => {
            for l in s.pending() {
                println!("{}", row(&l));
            }
        }
        MemoryCmd::Accept { id } => {
            let n = s.accept(&id)?;
            anyhow::ensure!(n > 0, "no single pending lesson with id {id}");
            println!("Accepted {n}.");
        }
        MemoryCmd::Reject { id } => {
            let one = s
                .pending()
                .into_iter()
                .filter(|l| l.id.starts_with(id.trim()))
                .count()
                == 1;
            anyhow::ensure!(
                one && s.forget(&id)?,
                "no single pending lesson with id {id}"
            );
            println!("Rejected {id}.");
        }
        MemoryCmd::Exam {
            current,
            candidate,
            margin,
            accept,
        } => {
            let read = |p: &std::path::Path| -> anyhow::Result<crate::bench::stats::Summary> {
                Ok(serde_json::from_str(&std::fs::read_to_string(p)?)?)
            };
            let (a, b) = (read(&current)?, read(&candidate)?);
            let pairs: Vec<exam::Paired> = a
                .tasks
                .iter()
                .filter_map(|t| {
                    let other = b.tasks.iter().find(|u| u.id == t.id)?;
                    Some(exam::Paired {
                        task: t.id.clone(),
                        current: t.pass,
                        candidate: other.pass,
                    })
                })
                .collect();
            let v = exam::judge(&pairs, (a.false_done as u32, b.false_done as u32), margin);
            println!(
                "{} paired tasks: {} wins {:?}, {} losses {:?}; false done {} -> {}; needs net +{}: {}",
                v.tasks,
                v.wins.len(),
                v.wins,
                v.losses.len(),
                v.losses,
                v.false_done_current,
                v.false_done_candidate,
                v.margin,
                if v.pass { "PASS" } else { "FAIL" }
            );
            if accept && v.pass {
                println!("Accepted {} pending lessons.", s.accept("all")?);
            }
            if !v.pass {
                return Ok(ExitCode::from(1));
            }
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

/// Every saved run under `dir` (the CLI's `<time>.json` files and the app's
/// `<time>/trace.json` folders). Unreadable files are skipped.
fn saved_traces(dir: &std::path::Path) -> Vec<kai_core::Trace> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        let file = if p.is_dir() { p.join("trace.json") } else { p };
        if file.extension().is_some_and(|x| x == "json")
            && let Ok(text) = std::fs::read_to_string(&file)
            && let Ok(t) = serde_json::from_str::<kai_core::Trace>(&text)
        {
            out.push(t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kai_core::memory::Source;

    #[test]
    fn saved_runs_are_read_in_both_layouts() {
        let dir = std::env::temp_dir().join(format!("kai-saved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("1700000000000")).unwrap();
        let t = serde_json::to_string(&kai_core::Trace::new("save it")).unwrap();
        std::fs::write(dir.join("1700000000.json"), &t).unwrap(); // kai run
        std::fs::write(dir.join("1700000000000/trace.json"), &t).unwrap(); // the app
        std::fs::write(dir.join("junk.json"), "not a trace").unwrap();
        assert_eq!(saved_traces(&dir).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
