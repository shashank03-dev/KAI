//! History: past runs from the local traces folder, each as a timeline of where
//! its time went and the frames Kai saw, exportable as a zip for bug reports.

use crate::app::KaiApp;
use crate::ui;
use ely_gpui_component::{
    agent::BrowserPreview,
    buttons::{Button, ButtonVariant},
    chat::{Conversation, ConversationList},
    debug::{Span, TimelineProfiler},
    feedback::Toast,
    forms::SearchInput,
    layout::ScrollArea,
    primitives::{IconName, Severity},
    theme::{ActiveTheme, TextSize},
};
use gpui::{Context, IntoElement, ParentElement, Styled, Window, div, px};
use kai_core::Trace;
use kai_core::settings::Settings;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Entry {
    pub dir: PathBuf,
    pub task: String,
    pub at: jiff::Timestamp,
}

/// Every run folder with a readable trace, newest first.
pub fn load() -> Vec<Entry> {
    let mut v: Vec<Entry> = std::fs::read_dir(Settings::traces_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let trace: Trace =
                serde_json::from_str(&std::fs::read_to_string(dir.join("trace.json")).ok()?)
                    .ok()?;
            let ms: i64 = dir.file_name()?.to_str()?.parse().ok()?;
            Some(Entry {
                dir,
                task: trace.task,
                at: jiff::Timestamp::from_millisecond(ms).ok()?,
            })
        })
        .collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.at));
    v
}

fn frames(dir: &Path) -> Vec<String> {
    let mut f: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("jpg"))
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    f.sort();
    f
}

/// One track per phase of a step, laid end to end on a millisecond axis.
pub fn spans(trace: &Trace) -> (Vec<Span>, f64) {
    let mut out = Vec::new();
    let mut t = 0.0;
    for s in &trace.steps {
        let tm = s.timings;
        let observe = tm.capture_ms.max(tm.tree_ms) as f64;
        let parts = [
            (0, "capture", tm.capture_ms as f64, 0.0),
            (1, "tree", tm.tree_ms as f64, 0.0),
            (2, "encode", tm.encode_ms as f64, observe),
            (
                3,
                "model",
                tm.model_ms as f64,
                observe + tm.encode_ms as f64,
            ),
            (
                4,
                "act",
                tm.act_ms as f64,
                observe + (tm.encode_ms + tm.model_ms) as f64,
            ),
        ];
        for (track, name, len, offset) in parts {
            if len > 0.0 {
                out.push(Span {
                    track,
                    name: format!("{name} · step {}", s.index + 1).into(),
                    start: t + offset,
                    end: t + offset + len,
                });
            }
        }
        t += tm.total_ms() as f64;
    }
    (out, t.max(1.0))
}

pub fn export(dir: &Path) -> anyhow::Result<PathBuf> {
    let name = format!(
        "kai-trace-{}.zip",
        dir.file_name().and_then(|n| n.to_str()).unwrap_or("run")
    );
    let out = dirs_download().join(name);
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&out)?);
    let opts = zip::write::SimpleFileOptions::default();
    for e in std::fs::read_dir(dir)?.flatten() {
        let p = e.path();
        if p.is_file() {
            zip.start_file(
                p.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
                opts,
            )?;
            zip.write_all(&std::fs::read(&p)?)?;
        }
    }
    zip.finish()?;
    Ok(out)
}

fn dirs_download() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let d = home.join("Downloads");
    if d.is_dir() { d } else { home }
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let theme = cx.theme();
    let q = app.pages.history_search.read(cx).text().to_string();
    let today = jiff::Zoned::now().date();
    let convs: Vec<Conversation> = app
        .pages
        .history
        .iter()
        .map(|e| Conversation {
            key: e.dir.to_string_lossy().to_string().into(),
            title: e.task.clone().into(),
            at: e.at,
            pinned: false,
        })
        .collect();
    let picker = cx.entity();
    let active = app
        .pages
        .open_trace
        .as_ref()
        .map(|p| p.to_string_lossy().to_string().into());
    let list =
        ConversationList::new("runs", convs, today)
            .query(q)
            .active(active, move |key, _, cx| {
                picker.update(cx, |a, cx| {
                    a.pages.open_trace = Some(PathBuf::from(key.as_ref()));
                    a.pages.trace_range = None;
                    cx.notify();
                })
            });

    let detail = app.pages.open_trace.clone().and_then(|dir| {
        let trace: Trace =
            serde_json::from_str(&std::fs::read_to_string(dir.join("trace.json")).ok()?).ok()?;
        let (spans, end) = spans(&trace);
        let range = app.pages.trace_range.unwrap_or((0.0, end));
        let (pan, exporter) = (cx.entity(), cx.entity());
        let shots = frames(&dir);
        let (p50, p95) = trace.step_percentiles();
        let d2 = dir.clone();
        Some(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_size(theme.text_size(TextSize::Md))
                        .child(trace.task.clone()),
                )
                .child(
                    div()
                        .text_size(theme.text_size(TextSize::Sm))
                        .text_color(theme.colors.fg_muted)
                        .child(format!(
                            "{} · {} steps · step p50 {p50} ms · p95 {p95} ms",
                            trace
                                .outcome
                                .map(crate::state::outcome_text)
                                .unwrap_or("Ended without a result"),
                            trace.steps.len()
                        )),
                )
                .child(
                    div().h_56().child(
                        TimelineProfiler::new(
                            "trace",
                            ["capture", "tree", "encode", "model", "act"],
                            spans,
                            range,
                        )
                        .on_range(move |r, _, cx| {
                            pan.update(cx, |a, cx| {
                                a.pages.trace_range = Some(r);
                                cx.notify();
                            })
                        }),
                    ),
                )
                .children((!shots.is_empty()).then(|| {
                    BrowserPreview::new("trace-frames", "Frames", shots.clone(), shots.len() - 1)
                }))
                .child(
                    Button::new("export", "Export as zip")
                        .variant(ButtonVariant::Secondary)
                        .icon(IconName::Download)
                        .on_click(move |_, _, cx| {
                            exporter.update(cx, |a, cx| match export(&d2) {
                                Ok(p) => a.toast(
                                    Toast::new("Trace exported")
                                        .body(p.display().to_string())
                                        .severity(Severity::Success),
                                    cx,
                                ),
                                Err(e) => a.toast(
                                    Toast::new("Export failed")
                                        .body(e.to_string())
                                        .severity(Severity::Danger),
                                    cx,
                                ),
                            })
                        }),
                ),
        )
    });

    let body = if app.pages.history.is_empty() {
        let starter = cx.entity();
        ui::empty(
            "No runs yet",
            "Saved runs appear here with their timelines and frames. Recording is controlled in Settings › Privacy.",
            Button::new("history-first-task", "Give Kai a task")
                .variant(ButtonVariant::Primary)
                .icon(IconName::Play)
                .on_click(move |_, _, cx| starter.update(cx, |a, cx| a.select(None, cx))),
            cx,
        )
        .into_any_element()
    } else {
        div()
            .flex()
            .gap_4()
            .size_full()
            .child(
                div()
                    .w_72()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(SearchInput::new(
                        "history-search",
                        &app.pages.history_search,
                    ))
                    .child(div().flex_1().min_h_0().child(list)),
            )
            .child(
                div().flex_1().min_w_0().h_full().child(
                    ScrollArea::new("trace-detail").size_full()
                        .vertical()
                        .child(div().w_full().child(detail.unwrap_or_else(|| {
                            div()
                                .pt_8()
                                .w_full()
                                .text_center()
                                .text_color(theme.colors.fg_muted)
                                .child("Pick a run on the left to see where its time went and the frames Kai saw.")
                        }))),
                ),
            )
            .into_any_element()
    };
    let n = app.pages.history.len();
    div()
        .size_full()
        .flex()
        .flex_col()
        .gap(px(24.))
        .px(px(40.))
        .pt(px(36.))
        .pb(px(24.))
        .child(ui::page(
            &format!("On this computer · {n} runs"),
            "History",
            "Past runs, kept only on this computer.",
            None,
            cx,
        ))
        .child(div().flex_1().min_h_0().child(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_lay_steps_end_to_end() {
        let mut t = Trace::new("x");
        let d =
            kai_core::Decision::parse_checked(r#"{"thought":"","action":"wait","risk":"none"}"#)
                .unwrap();
        for _ in 0..2 {
            t.push(kai_core::trace::StepRecord {
                index: 0,
                decision: d.clone(),
                timings: kai_core::trace::Timings {
                    capture_ms: 20,
                    tree_ms: 100,
                    encode_ms: 40,
                    model_ms: 500,
                    first_token_ms: None,
                    act_ms: 10,
                    verify_ms: 0,
                },
                backend: String::new(),
                elements: String::new(),
                screenshot: None,
                approval: None,
                verified: true,
                output: None,
                path: None,
                call: Default::default(),
                executor: None,
                effect: None,
                target: None,
                screen: None,
            });
        }
        let (spans, end) = spans(&t);
        assert_eq!(end, 1300.0);
        let model = spans.iter().find(|s| s.track == 3).unwrap();
        assert_eq!((model.start, model.end), (140.0, 640.0));
    }
}
