//! Benchmarks: the committed results per backend, as the projector page.
//! Numbers come only from `bench/results/*.json` files a run wrote. The
//! reader is tolerant: any field may be missing or renamed slightly, and a
//! missing number shows as "not measured", never as zero.

use crate::app::KaiApp;
use crate::ui;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    charts::{BarChart, Series},
    data_display::{Statistic, Tone},
    feedback::{Callout, EmptyState, Toast},
    forms::Choice,
    layout::ScrollArea,
    motion::ProgressBar,
    navigation::Tabs,
    primitives::{IconName, Severity},
    tables::{Cell, Column, DataTable, Row},
    theme::{ActiveTheme, TextSize},
};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, Window, div, rems};
use serde_json::Value;
use std::path::PathBuf;

const NOT_MEASURED: &str = "not measured";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub pass: Option<bool>,
    pub outcome: Option<String>,
    pub judged_by: Option<String>,
    pub steps: Option<f64>,
    pub seconds: Option<f64>,
    pub step_p50_ms: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Results {
    pub file: String,
    pub date: String,
    pub backend: String,
    pub model: Option<String>,
    pub os: Option<String>,
    pub note: Option<String>,
    pub tasks: Vec<TaskRow>,
    pub success_rate: Option<f64>,
    pub step_p50_ms: Option<f64>,
    pub step_p95_ms: Option<f64>,
    pub total_seconds: Option<f64>,
}

impl Results {
    /// "replay", or "gateway · google/gemma-4-26b-a4b-it".
    pub fn name(&self) -> String {
        match &self.model {
            Some(m) => format!("{} · {m}", self.backend),
            None => self.backend.clone(),
        }
    }
}

/// The first of `keys` present as a string.
fn text(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match v.get(*k)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

/// The first of `keys` present as a finite number.
fn number(v: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|k| v.get(*k)?.as_f64())
        .filter(|n| n.is_finite())
}

fn task(v: &Value, ix: usize) -> TaskRow {
    let outcome = text(v, &["outcome", "result", "status"]);
    let pass = v
        .get("pass")
        .or_else(|| v.get("passed"))
        .or_else(|| v.get("success"))
        .and_then(Value::as_bool)
        .or_else(|| outcome.as_deref().map(|o| o == "success" || o == "pass"));
    TaskRow {
        id: text(v, &["id", "task"]).unwrap_or_else(|| format!("task-{ix}")),
        title: text(v, &["title", "name", "id", "task"]).unwrap_or_default(),
        pass,
        outcome,
        judged_by: text(v, &["judged_by", "judge"]),
        steps: number(v, &["steps", "step_count"]),
        seconds: number(v, &["seconds", "total_seconds", "wall_seconds"]),
        step_p50_ms: number(v, &["step_p50_ms", "p50_ms"]),
    }
}

/// Read one results file; None if it isn't a JSON object.
pub fn parse(file: &str, json: &str) -> Option<Results> {
    let v: Value = serde_json::from_str(json).ok()?;
    v.as_object()?;
    let tasks: Vec<TaskRow> = v
        .get("tasks")
        .or_else(|| v.get("rows"))
        .or_else(|| v.get("results"))
        .and_then(Value::as_array)
        .map(|a| a.iter().enumerate().map(|(i, t)| task(t, i)).collect())
        .unwrap_or_default();
    Some(Results {
        file: file.to_string(),
        date: text(&v, &["date", "when", "timestamp"]).unwrap_or_default(),
        backend: text(&v, &["backend"]).unwrap_or_else(|| file.to_string()),
        model: text(&v, &["model", "model_id"]),
        os: text(&v, &["os", "platform"]),
        note: text(&v, &["note", "notes", "description"]),
        success_rate: number(&v, &["success_rate", "pass_rate"]),
        step_p50_ms: number(&v, &["step_p50_ms", "p50_ms"]),
        step_p95_ms: number(&v, &["step_p95_ms", "p95_ms"]),
        total_seconds: number(&v, &["total_seconds", "seconds"]),
        tasks,
    })
}

fn results_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [
        PathBuf::from("bench/results"),
        exe.join("bench/results"),
        exe.join("../../bench/results"),
    ]
    .into_iter()
    .find(|p| p.is_dir())
}

/// The newest results file per backend and model.
pub fn load() -> Vec<Results> {
    let Some(dir) = results_dir() else {
        return Vec::new();
    };
    let mut all: Vec<Results> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            parse(&name, &std::fs::read_to_string(e.path()).ok()?)
        })
        .collect();
    // Newest first; the file name breaks ties (it usually starts with the date).
    all.sort_by(|a, b| b.date.cmp(&a.date).then(b.file.cmp(&a.file)));
    let mut seen = std::collections::BTreeSet::new();
    all.retain(|r| seen.insert(r.name()));
    all
}

fn run_bench(app: &mut KaiApp, cx: &mut Context<KaiApp>) {
    let Some(exe) = std::env::current_exe().ok().and_then(|p| {
        p.parent()
            .map(|d| d.join(if cfg!(windows) { "kai.exe" } else { "kai" }))
    }) else {
        return;
    };
    app.pages.bench_running = true;
    cx.notify();
    cx.spawn(async move |this, cx| {
        let out = cx
            .background_executor()
            .spawn(async move {
                std::process::Command::new(exe)
                    .args(["bench", "--backend", "mock"])
                    .output()
            })
            .await;
        let _ = this.update(cx, |a, cx| {
            a.pages.bench_running = false;
            a.pages.bench = load();
            let toast = match out {
                Ok(o) if o.status.success() => {
                    Toast::new("Benchmark finished").severity(Severity::Success)
                }
                Ok(o) => Toast::new("Benchmark failed")
                    .body(
                        String::from_utf8_lossy(&o.stderr)
                            .chars()
                            .take(300)
                            .collect::<String>(),
                    )
                    .severity(Severity::Danger),
                Err(e) => Toast::new("Couldn't start the benchmark")
                    .body(e.to_string())
                    .severity(Severity::Danger),
            };
            a.toast(toast, cx);
            cx.notify();
        });
    })
    .detach();
}

fn num(v: Option<f64>) -> Cell {
    v.map(Cell::Number)
        .unwrap_or_else(|| Cell::Text(NOT_MEASURED.into()))
}

fn ms(v: Option<f64>) -> String {
    v.map(|v| format!("{v:.0} ms"))
        .unwrap_or_else(|| NOT_MEASURED.into())
}

/// One row per backend: what each measured.
fn overview(results: &[Results]) -> DataTable {
    let columns = vec![
        // Fixed widths: on a narrow window the table scrolls sideways with
        // the backend pinned, instead of cutting names and dates short.
        Column::new("backend", "Backend · model")
            .width(rems(15.))
            .pinned(),
        Column::new("date", "Date").width(rems(7.)),
        Column::new("rate", "Success").width(rems(5.5)),
        Column::new("p50", "Step p50").width(rems(6.5)),
        Column::new("p95", "Step p95").width(rems(6.5)),
        Column::new("total", "Total").width(rems(6.)),
        Column::new("os", "OS").width(rems(5.)),
    ];
    let rows = results.iter().map(|r| {
        Row::new(
            r.file.clone(),
            [
                Cell::Text(r.name().into()),
                Cell::Text(r.date.clone().into()),
                Cell::Text(
                    r.success_rate
                        .map(|s| format!("{:.0}%", s * 100.0))
                        .unwrap_or_else(|| NOT_MEASURED.into())
                        .into(),
                ),
                Cell::Text(ms(r.step_p50_ms).into()),
                Cell::Text(ms(r.step_p95_ms).into()),
                Cell::Text(
                    r.total_seconds
                        .map(|s| format!("{s:.1} s"))
                        .unwrap_or_else(|| NOT_MEASURED.into())
                        .into(),
                ),
                Cell::Text(r.os.clone().unwrap_or_default().into()),
            ],
        )
    });
    DataTable::new("bench-overview", columns).rows(rows.collect::<Vec<_>>())
}

fn tasks(r: &Results) -> DataTable {
    let columns = vec![
        Column::new("task", "Task"),
        Column::new("result", "Result"),
        Column::new("steps", "Steps").end(),
        Column::new("secs", "Seconds").end().decimals(2),
        Column::new("p50", "Step p50 ms").end(),
        Column::new("by", "Judged by"),
    ];
    let rows: Vec<Row> = r
        .tasks
        .iter()
        .map(|t| {
            let result = match (t.pass, &t.outcome) {
                (Some(true), _) => Cell::Tag("pass".into(), Tone::Success),
                (Some(false), Some(o)) => Cell::Tag(format!("fail · {o}").into(), Tone::Danger),
                (Some(false), None) => Cell::Tag("fail".into(), Tone::Danger),
                (None, Some(o)) => Cell::Tag(o.clone().into(), Tone::Neutral),
                (None, None) => Cell::Text(NOT_MEASURED.into()),
            };
            Row::new(
                t.id.clone(),
                [
                    Cell::Text(t.title.clone().into()),
                    result,
                    num(t.steps),
                    num(t.seconds),
                    num(t.step_p50_ms),
                    Cell::Text(t.judged_by.clone().unwrap_or_default().into()),
                ],
            )
        })
        .collect();
    DataTable::new(
        gpui::SharedString::from(format!("bench-{}", r.file)),
        columns,
    )
    .rows(rows)
}

fn chart(results: &[Results]) -> Option<AnyElement> {
    let measured: Vec<&Results> = results.iter().filter(|r| r.step_p50_ms.is_some()).collect();
    if measured.is_empty() {
        return None;
    }
    let mut chart = BarChart::new(
        "per-backend",
        measured.iter().map(|r| r.name()).collect::<Vec<_>>(),
    )
    .series(Series::new(
        "p50 ms",
        measured.iter().map(|r| r.step_p50_ms.unwrap_or_default()),
    ));
    // A second series only when every backend measured it: no made-up zeros.
    if measured.iter().all(|r| r.step_p95_ms.is_some()) {
        chart = chart.series(Series::new(
            "p95 ms",
            measured.iter().map(|r| r.step_p95_ms.unwrap_or_default()),
        ));
    }
    Some(div().h_64().child(chart).into_any_element())
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let theme = cx.theme();
    let runner = cx.entity();
    let run = Button::new("bench-run", "Run benchmark (mock)")
        .variant(ButtonVariant::Primary)
        .icon(IconName::Play)
        .loading(app.pages.bench_running)
        .on_click(move |_, _, cx| runner.update(cx, run_bench));
    let results = &app.pages.bench;
    if results.is_empty() {
        let content = ui::page(
            "Measured, not estimated",
            "Benchmarks",
            "No result files yet.",
            Some(run.into_any_element()),
            cx,
        )
        .child(
            EmptyState::new("no-bench", IconName::ChartColumn, "No benchmark results yet")
                .body("Run `kai bench --backend vllm` (or gemini, gateway) on this machine; results land in bench/results."),
        );
        return ScrollArea::new("bench")
            .size_full()
            .vertical()
            .child(ui::body(1040., content))
            .into_any_element();
    }

    let picked = results
        .iter()
        .find(|r| r.file == app.pages.bench_pick)
        .unwrap_or(&results[0]);
    let stats = div()
        .flex()
        .flex_wrap()
        .gap_6()
        .children(picked.success_rate.map(|s| {
            Statistic::new("rate", "Success rate", s * 100.0)
                .decimals(0)
                .suffix("%")
        }))
        .children(
            picked
                .step_p50_ms
                .map(|v| Statistic::new("p50", "Step p50", v).suffix(" ms")),
        )
        .children(
            picked
                .step_p95_ms
                .map(|v| Statistic::new("p95", "Step p95", v).suffix(" ms")),
        )
        .children(
            picked
                .total_seconds
                .map(|v| Statistic::new("total", "Total", v).decimals(1).suffix(" s")),
        );
    let picker = cx.entity();
    let tabs = Tabs::new(
        "bench-tabs",
        results
            .iter()
            .map(|r| Choice::new(r.file.clone(), r.name())),
        picked.file.clone(),
    )
    .on_change(move |v, _, cx| {
        picker.update(cx, |a, cx| {
            a.pages.bench_pick = v.to_string();
            cx.notify();
        })
    })
    .panel(
        div()
            .pt_3()
            .flex()
            .flex_col()
            .gap_4()
            .children(picked.note.clone().map(|n| {
                Callout::new(Severity::Info)
                    .title("What these numbers are")
                    .child(n)
            }))
            .child(stats)
            .child(tasks(picked)),
    );

    let content = ui::page(
        "Measured, not estimated",
        "Benchmarks",
        format!(
            "{} result files · newest per backend and model",
            results.len()
        ),
        Some(run.into_any_element()),
        cx,
    );
    ScrollArea::new("bench")
        .size_full()
        .vertical()
        .child(ui::body(
            1040.,
            content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .children(
                        app.pages
                            .bench_running
                            .then(|| ProgressBar::indeterminate("bench-progress")),
                    )
                    .child(overview(results))
                    .child(
                        div()
                            .text_size(theme.text_size(TextSize::Sm))
                            .text_color(theme.colors.fg_muted)
                            .child("Step time per backend (ms); only backends that measured it"),
                    )
                    .children(chart(results))
                    .child(tabs),
            ),
        ))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_current_format() {
        let r = parse(
            "2026-10-09-replay.json",
            r#"{"date":"2026-10-09","backend":"replay","os":"linux","note":"n",
               "tasks":[{"id":"a","title":"A","outcome":"success","pass":true,"judged_by":"check",
                         "steps":2,"seconds":0.47,"step_p50_ms":64,"step_p95_ms":64}],
               "success_rate":1.0,"step_p50_ms":64,"step_p95_ms":66,"total_seconds":3.5}"#,
        )
        .unwrap();
        assert_eq!(r.backend, "replay");
        assert_eq!(r.step_p95_ms, Some(66.0));
        assert_eq!(r.tasks[0].pass, Some(true));
    }

    #[test]
    fn missing_numbers_stay_missing() {
        let r = parse(
            "x.json",
            r#"{"backend":"gateway","model":"google/gemma-4-26b-a4b-it",
               "rows":[{"task":"t1","outcome":"failed"},{"id":"t2","seconds":null}]}"#,
        )
        .unwrap();
        assert_eq!(r.name(), "gateway · google/gemma-4-26b-a4b-it");
        assert_eq!(r.success_rate, None);
        assert_eq!(r.tasks.len(), 2);
        assert_eq!(r.tasks[0].pass, Some(false));
        assert_eq!(r.tasks[1].seconds, None);
        assert_eq!(r.tasks[1].pass, None);
    }

    #[test]
    fn junk_is_skipped() {
        assert!(parse("a.json", "[1,2]").is_none());
        assert!(parse("a.json", "not json").is_none());
        let r = parse("b.json", "{}").unwrap();
        assert_eq!(r.backend, "b.json");
    }
}
