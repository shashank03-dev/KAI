//! Benchmarks: the committed results per backend, as the projector page.
//! Numbers come only from `bench/results/*.json` files a run wrote.

use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    charts::{BarChart, Series},
    data_display::{Statistic, Tone},
    feedback::{Callout, EmptyState, Toast},
    layout::{Page, ScrollArea},
    motion::ProgressBar,
    primitives::{IconName, Severity},
    tables::{Cell, Column, DataTable, Row},
    theme::{ActiveTheme, TextSize},
};
use gpui::{Context, IntoElement, ParentElement, Styled, Window, div};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub pass: bool,
    pub judged_by: String,
    pub steps: usize,
    pub seconds: f64,
    pub step_p50_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Results {
    pub date: String,
    pub backend: String,
    pub os: String,
    pub note: String,
    pub tasks: Vec<TaskRow>,
    pub success_rate: f64,
    pub step_p50_ms: u64,
    pub step_p95_ms: u64,
    pub total_seconds: f64,
}

fn results_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [PathBuf::from("bench/results"), exe.join("bench/results"), exe.join("../../bench/results")].into_iter().find(|p| p.is_dir())
}

/// The newest results file per backend.
pub fn load() -> Vec<Results> {
    let Some(dir) = results_dir() else { return Vec::new() };
    let mut all: Vec<Results> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok())
        .collect();
    all.sort_by(|a, b| b.date.cmp(&a.date));
    let mut seen = std::collections::BTreeSet::new();
    all.retain(|r| seen.insert(r.backend.clone()));
    all
}

fn run_bench(app: &mut KaiApp, cx: &mut Context<KaiApp>) {
    let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(if cfg!(windows) { "kai.exe" } else { "kai" }))) else { return };
    app.pages.bench_running = true;
    cx.notify();
    cx.spawn(async move |this, cx| {
        let out = cx.background_executor().spawn(async move { std::process::Command::new(exe).args(["bench", "--backend", "mock"]).output() }).await;
        let _ = this.update(cx, |a, cx| {
            a.pages.bench_running = false;
            a.pages.bench = load();
            match out {
                Ok(o) if o.status.success() => a.toast(Toast::new("Benchmark finished").severity(Severity::Success), cx),
                Ok(o) => a.toast(Toast::new("Benchmark failed").body(String::from_utf8_lossy(&o.stderr).chars().take(300).collect::<String>()).severity(Severity::Danger), cx),
                Err(e) => a.toast(Toast::new("Couldn't start the benchmark").body(e.to_string()).severity(Severity::Danger), cx),
            }
            cx.notify();
        });
    })
    .detach();
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let theme = cx.theme();
    let runner = cx.entity();
    let run = Button::new("bench-run", "Run benchmark (mock)").variant(ButtonVariant::Primary).icon(IconName::Play).loading(app.pages.bench_running).on_click(move |_, _, cx| {
        runner.update(cx, |a, cx| run_bench(a, cx))
    });
    let results = &app.pages.bench;
    if results.is_empty() {
        return Page::new("Benchmarks").action(run).child(EmptyState::new("no-bench", IconName::ChartColumn, "No benchmark results yet").body("Run `kai bench --backend vllm` (or gemini) on this machine; results land in bench/results.")).into_any_element();
    }

    let latest = &results[0];
    let stats = div().flex().flex_wrap().gap_6()
        .child(Statistic::new("rate", "Success rate", latest.success_rate * 100.0).decimals(0).suffix("%"))
        .child(Statistic::new("p50", "Step p50", latest.step_p50_ms as f64).suffix(" ms"))
        .child(Statistic::new("p95", "Step p95", latest.step_p95_ms as f64).suffix(" ms"))
        .child(Statistic::new("total", "Total", latest.total_seconds).decimals(1).suffix(" s"));

    let chart = BarChart::new("per-backend", results.iter().map(|r| r.backend.clone()).collect::<Vec<_>>())
        .series(Series::new("p50 ms", results.iter().map(|r| r.step_p50_ms as f64)))
        .series(Series::new("p95 ms", results.iter().map(|r| r.step_p95_ms as f64)));

    let columns = vec![Column::new("task", "Task"), Column::new("result", "Result"), Column::new("steps", "Steps").end(), Column::new("secs", "Seconds").end().decimals(2), Column::new("p50", "Step p50 ms").end(), Column::new("by", "Judged by")];
    let rows: Vec<Row> = latest.tasks.iter().map(|t| {
        Row::new(t.id.clone(), [
            Cell::Text(t.title.clone().into()),
            Cell::Tag(if t.pass { "pass" } else { "fail" }.into(), if t.pass { Tone::Success } else { Tone::Danger }),
            Cell::Number(t.steps as f64),
            Cell::Number(t.seconds),
            Cell::Number(t.step_p50_ms as f64),
            Cell::Text(t.judged_by.clone().into()),
        ])
    }).collect();

    Page::new("Benchmarks")
        .subtitle(format!("Latest: {} on {} · {}", latest.backend, latest.os, latest.date))
        .action(run)
        .child(ScrollArea::new("bench").vertical().child(div().flex().flex_col().gap_5()
            .child(Callout::new(Severity::Info).title("What these numbers are").child(latest.note.clone()))
            .children(app.pages.bench_running.then(|| ProgressBar::indeterminate("bench-progress")))
            .child(stats)
            .child(div().text_size(theme.text_size(TextSize::Sm)).text_color(theme.colors.fg_muted).child("Step time per backend (ms)"))
            .child(div().h_64().child(chart))
            .child(DataTable::new("bench-table", columns).rows(rows))))
        .into_any_element()
}
