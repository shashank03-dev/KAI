//! Recording bench runs as training data: every task run becomes
//! `bench/runs/<date>-<backend>-<model>/<task-id>/` (see `docs/DATASET.md`),
//! with the automatic check and a failure label in its `meta.json`.

use super::report::slug;
use super::tasks::Task;
use kai_core::failure::{self, Category, Failure};
use kai_core::{RunMeta, Trace, TraceRecorder};
use std::path::{Path, PathBuf};

/// Where and as what a bench run is recorded.
pub struct Recording {
    /// `bench/runs/<date>-<backend>-<model>`.
    pub batch_dir: PathBuf,
    pub backend: String,
    pub model: String,
    pub careful_model: Option<String>,
    pub kai_version: String,
}

impl Recording {
    pub fn new(root: &Path, date: &str, backend: &str, model: &str) -> Self {
        let name = if model.is_empty() {
            format!("{date}-{}", slug(backend))
        } else {
            format!("{date}-{}-{}", slug(backend), slug(model))
        };
        Self {
            batch_dir: root.join(name),
            backend: backend.to_string(),
            model: model.to_string(),
            careful_model: None,
            kai_version: kai_version(),
        }
    }

    /// A recorder for one task run. A second run of the same task in the
    /// same batch goes to `<task-id>.2`, `.3`, … so nothing is overwritten.
    pub fn start(&self, task: &Task) -> Option<TraceRecorder> {
        let mut name = task.id.clone();
        let mut n = 1;
        while self.batch_dir.join(&name).exists() {
            n += 1;
            name = format!("{}.{n}", task.id);
        }
        let batch = self
            .batch_dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut meta = RunMeta::new(
            format!("{batch}/{name}"),
            &task.prompt,
            "bench",
            &self.backend,
            &self.model,
        );
        meta.task_id = Some(task.id.clone());
        meta.kai_version = self.kai_version.clone();
        match TraceRecorder::new(self.batch_dir.join(&name), meta) {
            Ok(r) => Some(match &self.careful_model {
                Some(m) => r.with_careful_model(m),
                None => r,
            }),
            Err(e) => {
                eprintln!("  (not recording {}: {e})", task.id);
                None
            }
        }
    }
}

/// The failure label of a bench task run: setup failures first, then the
/// trace heuristics. `None` when it passed.
pub fn label(trace: &Trace, check: Option<bool>, setup_failed: bool) -> Option<Failure> {
    if failure::passed(trace, check) {
        return None;
    }
    if setup_failed {
        return Some(Failure::new(Category::EnvError, "task setup failed"));
    }
    failure::classify(trace, check)
}

/// Write the trace (for runs the engine did not record itself, e.g. the
/// native loop) and the verdict.
pub fn finish(rec: &TraceRecorder, trace: &Trace, check: Option<bool>, setup_failed: bool) {
    let res = if rec.dir().join("trace.json").is_file() {
        Ok(())
    } else {
        rec.finish(trace)
    }
    .and_then(|_| rec.set_verdict(trace, check, label(trace, check, setup_failed)));
    if let Err(e) = res {
        eprintln!("  (recording failed: {e})");
    }
}

/// `git describe` when run inside the repo, else the crate version.
pub fn kai_version() -> String {
    std::process::Command::new("git")
        .args(["describe", "--always", "--dirty", "--tags"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}
