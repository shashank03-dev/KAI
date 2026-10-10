//! Results files (`bench/results/<date>-<backend>-<model>.{json,md}`), the
//! console table, and `kai bench report`, which merges results files into one
//! head-to-head Markdown table. The report only reads files that exist; it
//! never fills in a number that was not measured.

use super::stats::{Summary, failures_text};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// `google/gemma-4-26b-a4b-it` -> `google-gemma-4-26b-a4b-it`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

pub fn file_stem(s: &Summary) -> String {
    if s.model.is_empty() {
        format!("{}-{}", s.date, slug(&s.backend))
    } else {
        format!("{}-{}-{}", s.date, slug(&s.backend), slug(&s.model))
    }
}

/// Write `<stem>.json` and `<stem>.md`; returns both paths.
pub fn write(summary: &Summary, dir: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let stem = file_stem(summary);
    let json = dir.join(format!("{stem}.json"));
    let md = dir.join(format!("{stem}.md"));
    std::fs::write(&json, serde_json::to_string_pretty(summary)? + "\n")?;
    std::fs::write(&md, single_markdown(summary))?;
    Ok((json, md))
}

fn label(s: &Summary) -> String {
    if s.model.is_empty() {
        s.backend.clone()
    } else {
        format!("{} · {}", s.backend, s.model)
    }
}

fn opt_u64(v: Option<u64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "—".into())
}

fn cost(v: Option<f64>) -> String {
    v.map(|x| format!("${x:.4}")).unwrap_or_else(|| "—".into())
}

fn passed(s: &Summary) -> String {
    format!(
        "{} / {} ({:.0}%)",
        s.passed,
        s.total,
        s.success_rate * 100.0
    )
}

fn ab_text(s: &Summary) -> String {
    match &s.ab {
        None => "—".into(),
        Some(ab) => {
            let off = ab.flags.changed();
            let flags = if off.is_empty() {
                "all on".to_string()
            } else {
                off.join(" ")
            };
            if ab.applied {
                flags
            } else {
                format!("{flags} (engine has no switches; defaults)")
            }
        }
    }
}

fn executor_text(s: &Summary) -> String {
    if !s.executor {
        return "off".into();
    }
    format!(
        "on · {} decisions · {} escalations · {} / {} ms (p50 / p95)",
        s.executor_decisions, s.executor_escalations, s.executor_p50_ms, s.executor_p95_ms
    )
}

fn machine_text(s: &Summary) -> String {
    match &s.machine {
        None => s.os.clone(),
        Some(m) => format!("{} · {} cores · {} · {}", m.cpu, m.cores, m.os, m.display),
    }
}

/// The rows shared by the single-run and head-to-head tables.
fn metric_rows(s: &Summary) -> Vec<(&'static str, String)> {
    vec![
        ("Date", s.date.clone()),
        ("Tasks passed", passed(s)),
        ("Skipped", s.skipped.to_string()),
        (
            "Step p50 / p95 (ms)",
            format!("{} / {}", s.step_p50_ms, s.step_p95_ms),
        ),
        (
            "Model p50 / p95 (ms)",
            format!("{} / {}", s.model_p50_ms, s.model_p95_ms),
        ),
        (
            "Observe p50 / p95 (ms)",
            format!("{} / {}", s.observe_p50_ms, s.observe_p95_ms),
        ),
        (
            "Act p50 / p95 (ms)",
            format!("{} / {}", s.act_p50_ms, s.act_p95_ms),
        ),
        (
            "Seconds per task (mean)",
            format!("{:.2}", s.mean_seconds_per_task),
        ),
        (
            "Steps per task (mean)",
            format!("{:.2}", s.mean_steps_per_task),
        ),
        (
            "Model calls per task (mean)",
            format!("{:.2}", s.mean_model_calls_per_task),
        ),
        (
            "Prompt / completion tokens",
            format!(
                "{} / {}",
                opt_u64(s.prompt_tokens),
                opt_u64(s.completion_tokens)
            ),
        ),
        ("Cost (US$)", cost(s.cost_usd)),
        ("Failures", failures_text(&s.failures)),
        ("Total time (s)", format!("{:.1}", s.total_seconds)),
        ("A/B flags", ab_text(s)),
        ("Executor", executor_text(s)),
        ("Machine", machine_text(s)),
    ]
}

fn failure_cell(f: Option<&kai_core::Failure>) -> String {
    match f {
        None => "—".into(),
        Some(f) => esc(&format!("{}: {}", f.category, f.detail)),
    }
}

fn esc(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

pub fn single_markdown(s: &Summary) -> String {
    let mut md = String::new();
    let _ = writeln!(md, "# Kai bench: {}\n", label(s));
    let _ = writeln!(md, "{}\n", s.note);
    if !s.command.is_empty() {
        let _ = writeln!(md, "Command: `{}`\n", s.command);
    }
    let _ = writeln!(md, "| Metric | Value |\n|---|---|");
    for (k, v) in metric_rows(s) {
        let _ = writeln!(md, "| {k} | {} |", esc(&v));
    }
    let _ = writeln!(
        md,
        "\n| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |\n|---|---|---|---|---|---|---|---|---|---|"
    );
    for t in &s.tasks {
        let result = match (t.judged_by.as_str(), t.pass) {
            ("skipped", _) => format!(
                "skipped ({})",
                t.skip_reason.as_deref().unwrap_or("not run")
            ),
            (_, true) => "pass".into(),
            (_, false) => "FAIL".into(),
        };
        let _ = writeln!(
            md,
            "| {} | {:?} | {} | {} | {} | {:.2} | {} | {} | {} / {} | {} |",
            t.id,
            t.kind,
            esc(&result),
            t.judged_by,
            t.steps,
            t.seconds,
            t.step_p50_ms,
            t.model_calls,
            opt_u64(t.prompt_tokens),
            opt_u64(t.completion_tokens),
            failure_cell(t.failure.as_ref())
        );
    }
    let _ = writeln!(md, "\nFailure categories: {}", failures_text(&s.failures));
    if !s.missing.is_empty() {
        let _ = writeln!(md, "\nNot measured on this run:\n");
        for m in &s.missing {
            let _ = writeln!(md, "- {m}");
        }
    }
    md
}

/// Load every results file in `dir` and keep the latest per backend + model.
pub fn load_latest(dir: &Path) -> anyhow::Result<Vec<(PathBuf, Summary)>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    let mut latest: BTreeMap<(String, String), (PathBuf, Summary)> = BTreeMap::new();
    for p in files {
        let s: Summary = match std::fs::read_to_string(&p)
            .map_err(anyhow::Error::from)
            .and_then(|t| serde_json::from_str(&t).map_err(Into::into))
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("skipping {}: {e}", p.display());
                continue;
            }
        };
        if s.backend.is_empty() || s.tasks.is_empty() {
            eprintln!("skipping {}: not a kai bench results file", p.display());
            continue;
        }
        let key = (s.backend.clone(), s.model.clone());
        // Files are sorted by name, which starts with the date: later wins.
        let newer = latest.get(&key).is_none_or(|(_, old)| s.date >= old.date);
        if newer {
            latest.insert(key, (p, s));
        }
    }
    Ok(latest.into_values().collect())
}

/// The head-to-head table: one column per backend + model.
pub fn merged_markdown(runs: &[(PathBuf, Summary)]) -> String {
    let mut md = String::new();
    let _ = writeln!(md, "## Kai benchmark: head to head\n");
    let _ = writeln!(
        md,
        "Generated by `kai bench report` from {} results file(s), the latest per backend and model. Every number was measured by `kai bench`; \"—\" means not measured.\n",
        runs.len()
    );
    if runs.is_empty() {
        let _ = writeln!(md, "No results files found.");
        return md;
    }
    let cols: Vec<&Summary> = runs.iter().map(|(_, s)| s).collect();
    let _ = write!(md, "| |");
    for s in &cols {
        let _ = write!(md, " {} |", esc(&label(s)));
    }
    let _ = write!(md, "\n|---|");
    for _ in &cols {
        let _ = write!(md, "---|");
    }
    md.push('\n');
    let rows: Vec<Vec<(&str, String)>> = cols.iter().map(|s| metric_rows(s)).collect();
    for (i, (name, _)) in rows[0].iter().enumerate() {
        let _ = write!(md, "| {name} |");
        for r in &rows {
            let _ = write!(md, " {} |", esc(&r[i].1));
        }
        md.push('\n');
    }

    // Per-task matrix, in first-seen order across files.
    let mut ids: Vec<(String, String)> = Vec::new();
    for s in &cols {
        for t in &s.tasks {
            if !ids.iter().any(|(id, _)| id == &t.id) {
                ids.push((t.id.clone(), format!("{:?}", t.kind).to_lowercase()));
            }
        }
    }
    let _ = write!(md, "\n### Per task\n\n| Task | Kind |");
    for s in &cols {
        let _ = write!(md, " {} |", esc(&label(s)));
    }
    let _ = write!(md, "\n|---|---|");
    for _ in &cols {
        let _ = write!(md, "---|");
    }
    md.push('\n');
    for (id, kind) in &ids {
        let _ = write!(md, "| {id} | {kind} |");
        for s in &cols {
            let cell = match s.tasks.iter().find(|t| &t.id == id) {
                None => "—".to_string(),
                Some(t) if t.judged_by == "skipped" => "skip".into(),
                Some(t) => format!(
                    "{} ({} steps, {:.1} s)",
                    if t.pass { "pass" } else { "FAIL" },
                    t.steps,
                    t.seconds
                ),
            };
            let _ = write!(md, " {cell} |");
        }
        md.push('\n');
    }

    let _ = writeln!(md, "\n### Notes\n");
    for (path, s) in runs {
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        let _ = writeln!(md, "- **{}** (`{file}`): {}", esc(&label(s)), s.note);
        if !s.command.is_empty() {
            let _ = writeln!(md, "  Command: `{}`", s.command);
        }
        for m in &s.missing {
            let _ = writeln!(md, "  Not measured: {m}");
        }
    }
    md
}

pub fn print(s: &Summary) {
    eprintln!(
        "\n{:<22} {:<8} {:>6} {:>8} {:>8} {:>8} {:>6} {:>13}",
        "task", "result", "steps", "secs", "p50 ms", "p95 ms", "calls", "tokens in/out"
    );
    for t in &s.tasks {
        let result = if t.judged_by == "skipped" {
            "skip"
        } else if t.pass {
            "pass"
        } else {
            "FAIL"
        };
        eprintln!(
            "{:<22} {:<8} {:>6} {:>8.2} {:>8} {:>8} {:>6} {:>13}",
            t.id,
            result,
            t.steps,
            t.seconds,
            t.step_p50_ms,
            t.step_p95_ms,
            t.model_calls,
            format!(
                "{}/{}",
                opt_u64(t.prompt_tokens),
                opt_u64(t.completion_tokens)
            )
        );
    }
    eprintln!(
        "\n{} passed, {} skipped · step p50 {} ms · p95 {} ms · model p50 {} ms · {:.1} s total · {}",
        passed(s),
        s.skipped,
        s.step_p50_ms,
        s.step_p95_ms,
        s.model_p50_ms,
        s.total_seconds,
        label(s)
    );
    eprintln!("failures: {}", failures_text(&s.failures));
    for t in &s.tasks {
        if let Some(f) = &t.failure {
            eprintln!("  {:<22} {}: {}", t.id, f.category, f.detail);
        }
    }
    for m in &s.missing {
        eprintln!("not measured: {m}");
    }
}
