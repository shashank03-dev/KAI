//! `kai data export`: recorded runs → JSONL (formats in `docs/DATASET.md`).

use super::Format;
use crate::bench::tasks::{PerOs, Task, load_tasks};
use kai_core::failure;
use kai_core::recorder::{RecordedRun, load_runs};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Which runs to keep.
#[derive(Debug, Clone, Copy, Default)]
pub struct Filter {
    pub only_success: bool,
    pub min_check: Option<bool>,
}

impl Filter {
    pub fn keep(&self, r: &RecordedRun) -> bool {
        let passed = failure::passed(&r.trace, r.meta.check);
        (!self.only_success || passed) && self.min_check.is_none_or(|c| r.meta.check == Some(c))
    }
}

/// One model call (formats `all` and `sft`).
#[derive(Debug, Serialize)]
pub struct StepRow {
    pub id: String,
    pub task: String,
    pub task_id: Option<String>,
    pub step: u32,
    pub system: String,
    pub user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_spec: Option<serde_json::Value>,
    pub images: Vec<String>,
    pub reply: String,
    pub decision: kai_core::Decision,
    pub run_outcome: Option<kai_core::Outcome>,
    pub check: Option<bool>,
    pub step_verified: bool,
    pub backend: String,
    pub model: String,
    pub latency_ms: u64,
    pub source: String,
}

/// One task for an RL environment (format `grpo`).
#[derive(Debug, Serialize)]
pub struct TaskRow {
    pub id: String,
    pub task_id: Option<String>,
    /// The prompt given to the model.
    pub task: String,
    pub kind: Option<String>,
    pub needs: Vec<String>,
    pub setup: Option<PerOsRow>,
    pub check: Option<PerOsRow>,
    pub teardown: Option<PerOsRow>,
    pub variant_of: Option<String>,
    /// Recorded runs of this task (after filters) and how many passed.
    pub runs: usize,
    pub passed: usize,
    pub failures: BTreeMap<String, usize>,
    /// Run directories, for reference trajectories.
    pub run_dirs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PerOsRow {
    pub linux: Option<String>,
    pub windows: Option<String>,
}

fn per_os(p: &PerOs) -> Option<PerOsRow> {
    (!p.is_empty()).then(|| PerOsRow {
        linux: p.linux.clone(),
        windows: p.windows.clone(),
    })
}

/// The step rows of one run (only steps that carry a model call).
pub fn step_rows(r: &RecordedRun) -> anyhow::Result<Vec<StepRow>> {
    let abs = |rel: &str| r.dir.join(rel).display().to_string();
    r.trace
        .steps
        .iter()
        .filter(|s| s.call.reply_raw.is_some())
        .map(|s| {
            let reply = s.call.reply_raw.clone().unwrap();
            let images = [s.screenshot.as_deref(), s.call.crop.as_deref()]
                .into_iter()
                .flatten()
                .map(abs)
                .collect();
            Ok(StepRow {
                id: format!("{}/{}", r.meta.run_id, s.index),
                task: r.meta.task.clone(),
                task_id: r.meta.task_id.clone(),
                step: s.index,
                system: r.system_for_step(s.call.prompt_system_hash.as_deref())?,
                user: s.call.prompt_user.clone().unwrap_or_default(),
                tool_spec: s.call.tool_spec.clone(),
                images,
                reply,
                decision: s.decision.clone(),
                run_outcome: r.trace.outcome,
                check: r.meta.check,
                step_verified: s.verified,
                backend: r.meta.backend.clone(),
                model: s.call.model.clone().unwrap_or_else(|| r.meta.model.clone()),
                latency_ms: s.timings.model_ms,
                source: r.meta.source.clone(),
            })
        })
        .collect()
}

/// One row per distinct task (by task id, else prompt) among `runs`.
pub fn task_rows(runs: &[&RecordedRun], tasks: &[Task]) -> Vec<TaskRow> {
    let mut rows: Vec<TaskRow> = Vec::new();
    for r in runs {
        let key = r
            .meta
            .task_id
            .clone()
            .unwrap_or_else(|| r.meta.task.clone());
        let i = match rows.iter().position(|t| t.id == key) {
            Some(i) => i,
            None => {
                let def = r
                    .meta
                    .task_id
                    .as_deref()
                    .and_then(|id| tasks.iter().find(|t| t.id == id));
                rows.push(TaskRow {
                    id: key,
                    task_id: r.meta.task_id.clone(),
                    task: def.map(|t| t.prompt.clone()).unwrap_or(r.meta.task.clone()),
                    kind: def.map(|t| format!("{:?}", t.kind).to_lowercase()),
                    needs: def.map(|t| t.needs.clone()).unwrap_or_default(),
                    setup: def.and_then(|t| per_os(&t.setup)),
                    check: def.and_then(|t| per_os(&t.check)),
                    teardown: def.and_then(|t| per_os(&t.teardown)),
                    variant_of: def.and_then(|t| t.variant_of.clone()),
                    runs: 0,
                    passed: 0,
                    failures: BTreeMap::new(),
                    run_dirs: Vec::new(),
                });
                rows.len() - 1
            }
        };
        let row = &mut rows[i];
        row.runs += 1;
        if failure::passed(&r.trace, r.meta.check) {
            row.passed += 1;
        } else if let Some(f) = run_failure(r) {
            *row.failures.entry(f.category.to_string()).or_insert(0) += 1;
        }
        row.run_dirs.push(r.dir.display().to_string());
    }
    rows
}

/// The run's failure label: from meta.json, else computed now.
pub fn run_failure(r: &RecordedRun) -> Option<kai_core::Failure> {
    if failure::passed(&r.trace, r.meta.check) {
        return None;
    }
    r.meta
        .failure
        .clone()
        .or_else(|| failure::classify(&r.trace, r.meta.check))
}

/// Load runs under `root` with absolute run paths.
pub fn load(root: &Path) -> anyhow::Result<Vec<RecordedRun>> {
    let root: PathBuf = std::fs::canonicalize(root)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", root.display()))?;
    Ok(load_runs(&root)?)
}

/// Write the export; returns the number of records.
pub fn export(
    input: &Path,
    out: &Path,
    format: Format,
    filter: Filter,
    task_files: &[PathBuf],
) -> anyhow::Result<usize> {
    let runs = load(input)?;
    let kept: Vec<&RecordedRun> = runs
        .iter()
        .filter(|r| filter.keep(r))
        .filter(|r| format != Format::Sft || failure::passed(&r.trace, r.meta.check))
        .collect();
    let mut lines: Vec<String> = Vec::new();
    if format == Format::Grpo {
        let mut tasks = Vec::new();
        for f in task_files {
            tasks.extend(load_tasks(f)?);
        }
        for row in task_rows(&kept, &tasks) {
            lines.push(serde_json::to_string(&row)?);
        }
    } else {
        for r in &kept {
            for row in step_rows(r)? {
                lines.push(serde_json::to_string(&row)?);
            }
        }
    }
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::io::BufWriter::new(std::fs::File::create(out)?);
    for l in &lines {
        writeln!(f, "{l}")?;
    }
    f.flush()?;
    Ok(lines.len())
}
