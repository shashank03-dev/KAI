//! Skills: search, cards, enable/disable, import, details, and Save as skill.

use crate::app::KaiApp;
use crate::ui::{self, Glass};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    data_display::{Badge, Tone},
    documents::{MarkdownEditor, MarkdownMode, MarkdownRenderer},
    feedback::{EmptyState, InlineMessage, Toast},
    forms::{DropZone, SearchInput, Switch},
    layout::{ScrollArea, Sheet},
    primitives::{IconName, Place, Severity},
    theme::{ActiveTheme, TextSize},
};
use gpui::{Context, InteractiveElement, IntoElement, ParentElement, Styled, Window, div, px};
use kai_core::Skill;
use kai_core::settings::Settings;
use std::path::Path;

pub fn render(app: &mut KaiApp, window: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let detail = app
        .pages
        .open_skill
        .clone()
        .and_then(|n| app.skills.iter().find(|s| s.front.name == n).cloned())
        .map(|s| detail_sheet(&s, cx));
    let draft = app.pages.drafting.then(|| draft_sheet(app, window, cx));
    let theme = cx.theme();
    let g = Glass::of(cx);
    let q = app.pages.skill_search.read(cx).text().to_ascii_lowercase();
    let shown: Vec<Skill> = app
        .skills
        .iter()
        .filter(|s| {
            q.is_empty()
                || s.front.name.contains(&q)
                || s.front.description.to_ascii_lowercase().contains(&q)
        })
        .cloned()
        .collect();
    let has_own = app.skills.iter().any(|s| !s.builtin);

    let cards = shown.iter().enumerate().map(|(ix, s)| {
        let name = s.front.name.clone();
        let on = !app.pages.disabled.contains(&name);
        let (toggle, open) = (cx.entity(), cx.entity());
        let (n1, n2) = (name.clone(), name.clone());
        let (hover, strong) = (g.card_hover, g.line_strong);
        let card = ui::card(&g)
            .id(gpui::SharedString::from(format!("skill-{name}")))
            .p(px(18.))
            .flex()
            .flex_col()
            .justify_between()
            .gap(px(10.))
            .opacity(if on { 1. } else { 0.6 })
            .hover(move |s| s.bg(hover).border_color(strong))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(theme.mono_family.clone())
                            .text_size(px(13.))
                            .text_color(g.text)
                            .truncate()
                            .child(name.clone()),
                    )
                    .child(
                        Badge::new(if s.builtin { "built-in" } else { "yours" })
                            .tone(if s.builtin { Tone::Neutral } else { Tone::Info }),
                    ),
            )
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Sm))
                    .text_color(theme.colors.fg_muted)
                    .line_clamp(3)
                    .child(s.front.description.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Switch::new(gpui::SharedString::from(format!("on-{name}")), on)
                            .label("Enabled")
                            .on_change(move |v, _, cx| {
                                toggle.update(cx, |a, cx| {
                                    if v {
                                        a.pages.disabled.remove(&n1);
                                    } else {
                                        a.pages.disabled.insert(n1.clone());
                                    }
                                    // Kept across restarts.
                                    a.settings.disabled_skills =
                                        a.pages.disabled.iter().cloned().collect();
                                    a.save_settings(cx);
                                })
                            }),
                    )
                    .child(
                        Button::new(gpui::SharedString::from(format!("open-{name}")), "Details")
                            .variant(ButtonVariant::Ghost)
                            .on_click(move |_, _, cx| {
                                open.update(cx, |a, cx| {
                                    a.pages.open_skill = Some(n2.clone());
                                    cx.notify();
                                })
                            }),
                    ),
            );
        crate::anim::enter(card, ("skill-in", ix), ix)
    });

    let importer = cx.entity();
    let import = DropZone::new("import")
        .hint("Drop a skill folder (with SKILL.md) to add it")
        .on_drop(move |paths, _, cx| importer.update(cx, |a, cx| import_skills(a, paths, cx)));

    let n = app.skills.len();
    let on = n - app
        .skills
        .iter()
        .filter(|s| app.pages.disabled.contains(&s.front.name))
        .count();
    let content = ui::page(
        &format!("Library · {on} of {n} on"),
        "Skills",
        "Agent Skill Open Standard skills Kai can use. Only names and descriptions go to the model until one is picked.",
        None,
        cx,
    )
    .child(
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(SearchInput::new("skill-search", &app.pages.skill_search))
            .child(div().grid().grid_cols(2).gap(px(12.)).children(cards))
            .children((!has_own).then(|| {
                EmptyState::new("no-own", IconName::Puzzle, "No skills of your own yet")
                    .body("Finish a run and press Save as skill, or drop a skill folder below.")
            }))
            .child(import),
    );
    div()
        .size_full()
        .child(
            ScrollArea::new("skills-page")
                .vertical()
                .child(ui::body(1040., content)),
        )
        .children(detail)
        .children(draft)
}

fn detail_sheet(s: &Skill, cx: &mut Context<KaiApp>) -> gpui::AnyElement {
    let closer = cx.entity();
    let mut files: Vec<String> = vec!["SKILL.md".into()];
    for sub in ["scripts", "references", "assets"] {
        if let Ok(rd) = std::fs::read_dir(s.dir.join(sub)) {
            files.extend(
                rd.flatten()
                    .map(|e| format!("{sub}/{}", e.file_name().to_string_lossy())),
            );
        }
    }
    let valid = match kai_core::skill::load(&s.dir) {
        Ok(_) => InlineMessage::new(
            Severity::Success,
            "Valid against the Agent Skill Open Standard.",
        ),
        Err(e) => InlineMessage::new(Severity::Danger, e.to_string()),
    };
    let md = std::fs::read_to_string(s.dir.join("SKILL.md")).unwrap_or_default();
    // The frontmatter is already shown above (name, description); left in,
    // Markdown reads its closing `---` as a heading underline.
    let md = body_of(&md).to_string();
    let theme = cx.theme();
    let file_list = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(theme.text_size(TextSize::Sm))
                .text_color(theme.colors.fg_muted)
                .child(format!("Files ({})", files.len())),
        )
        .children(files.into_iter().map(|f| {
            div()
                .text_size(theme.text_size(TextSize::Sm))
                .font_family(theme.mono_family.clone())
                .child(f)
        }));
    let description = div()
        .text_size(theme.text_size(TextSize::Sm))
        .text_color(theme.colors.fg_muted)
        .child(s.front.description.clone());
    Sheet::new(
        "skill-sheet",
        Place::Right,
        s.front.name.clone(),
        move |_, cx| {
            closer.update(cx, |a, cx| {
                a.pages.open_skill = None;
                cx.notify();
            })
        },
    )
    .child(
        ScrollArea::new("skill-detail").vertical().child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(description)
                .child(valid)
                .children((!s.steps.is_empty()).then(|| {
                    InlineMessage::new(
                        Severity::Info,
                        format!(
                            "{} deterministic steps run with no model call.",
                            s.steps.len()
                        ),
                    )
                }))
                .child(file_list)
                .child(MarkdownRenderer::new("skill-md", md)),
        ),
    )
    .into_any_element()
}

/// SKILL.md without its `---` frontmatter block.
pub fn body_of(md: &str) -> &str {
    let Some(rest) = md.strip_prefix("---") else {
        return md;
    };
    // The closing fence is a line of its own.
    match rest.find("\n---") {
        Some(end) => {
            let after = &rest[end + 4..];
            after
                .find('\n')
                .map(|nl| &after[nl + 1..])
                .unwrap_or("")
                .trim_start_matches(['\r', '\n'])
        }
        None => md,
    }
}

/// Draft a SKILL.md from the finished run and open the editor.
pub fn open_draft(app: &mut KaiApp, cx: &mut Context<KaiApp>) {
    let r = app.run.read(cx);
    let text = draft_from(&r.task, r.trace.as_ref());
    app.pages
        .skill_draft
        .update(cx, |f, cx| f.set_text(text, cx));
    app.pages.drafting = true;
    app.go(crate::app::Page::Skills, cx);
}

/// A spec-valid skill drawn from what worked: the URLs and shortcuts become
/// deterministic steps, the rest becomes instructions.
pub fn draft_from(task: &str, trace: Option<&kai_core::Trace>) -> String {
    let name = slug(task);
    let mut steps = Vec::new();
    let mut lines = Vec::new();
    for (i, s) in trace
        .map(|t| t.steps.as_slice())
        .unwrap_or(&[])
        .iter()
        .enumerate()
    {
        let d = &s.decision;
        let text = d.text.clone().unwrap_or_default();
        match d.action {
            kai_core::Action::OpenUrl if steps.len() == i => {
                steps.push(serde_json::json!({"do": "open_url", "url": text}))
            }
            kai_core::Action::OpenApp if steps.len() == i => {
                steps.push(serde_json::json!({"do": "open_app", "app": text}))
            }
            kai_core::Action::Key if steps.len() == i => {
                steps.push(serde_json::json!({"do": "shortcut", "keys": d.keys}))
            }
            kai_core::Action::Done => {}
            _ => lines.push(format!(
                "{}. {} — {}",
                lines.len() + 1,
                crate::state::describe(d, None),
                d.thought
            )),
        }
    }
    let meta = if steps.is_empty() {
        String::new()
    } else {
        format!("  kai: '{}'\n", serde_json::Value::Array(steps))
    };
    format!(
        "---\nname: {name}\ndescription: {desc}. Use when the user asks to {lower}.\nlicense: MIT\nmetadata:\n  author: you\n{meta}---\n# {title}\n\nWhat worked last time:\n\n{body}\n\nCheck the screen after each step and reply `done` when the task is finished.\n",
        desc = task.trim_end_matches('.'),
        lower = task.trim_end_matches('.').to_lowercase(),
        title = task.trim_end_matches('.'),
        body = if lines.is_empty() {
            "1. Follow the steps above.".to_string()
        } else {
            lines.join("\n")
        },
    )
}

/// A spec-valid name: lowercase words joined by single hyphens, at most 64.
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

fn draft_sheet(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> gpui::AnyElement {
    let text = app.pages.skill_draft.read(cx).text().to_string();
    let name = text
        .lines()
        .find_map(|l| l.strip_prefix("name: "))
        .unwrap_or("my-skill")
        .trim()
        .to_string();
    let check = kai_core::skill::load_from_str(&text, Path::new(&name));
    let msg = match &check {
        Ok(s) => InlineMessage::new(
            Severity::Success,
            format!("Valid skill “{}”.", s.front.name),
        ),
        Err(e) => InlineMessage::new(Severity::Danger, e.to_string()),
    };
    let (closer, saver) = (cx.entity(), cx.entity());
    let ok = check.is_ok();
    Sheet::new(
        "draft-sheet",
        Place::Right,
        "Save as skill",
        move |_, cx| {
            closer.update(cx, |a, cx| {
                a.pages.drafting = false;
                cx.notify();
            })
        },
    )
    .child(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(msg)
            .child(div().h_96().child(MarkdownEditor::new(
                "draft-editor",
                &app.pages.skill_draft,
                MarkdownMode::Split,
            )))
            .child(
                Button::new("draft-save", "Save skill")
                    .variant(ButtonVariant::Primary)
                    .disabled(!ok)
                    .on_click(move |_, _, cx| saver.update(cx, save_draft)),
            ),
    )
    .into_any_element()
}

fn save_draft(app: &mut KaiApp, cx: &mut Context<KaiApp>) {
    let text = app.pages.skill_draft.read(cx).text().to_string();
    let name = text
        .lines()
        .find_map(|l| l.strip_prefix("name: "))
        .unwrap_or("my-skill")
        .trim()
        .to_string();
    let dir = Settings::skills_dir().join(&name);
    let result =
        std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("SKILL.md"), &text));
    match result
        .map_err(anyhow::Error::from)
        .and_then(|_| kai_core::skill::load(&dir).map_err(anyhow::Error::from))
    {
        Ok(s) => {
            app.skills.retain(|x| x.front.name != s.front.name);
            app.skills.push(s);
            app.pages.drafting = false;
            app.toast(
                Toast::new("Skill saved")
                    .body(dir.display().to_string())
                    .severity(Severity::Success),
                cx,
            );
        }
        Err(e) => app.toast(
            Toast::new("Skill not saved")
                .body(e.to_string())
                .severity(Severity::Danger),
            cx,
        ),
    }
    cx.notify();
}

fn import_skills(app: &mut KaiApp, paths: Vec<std::path::PathBuf>, cx: &mut Context<KaiApp>) {
    for p in paths {
        match kai_core::skill::load(&p) {
            Ok(s) => {
                let dest = Settings::skills_dir().join(&s.front.name);
                if let Err(e) = copy_dir(&p, &dest) {
                    app.toast(
                        Toast::new("Import failed")
                            .body(e.to_string())
                            .severity(Severity::Danger),
                        cx,
                    );
                    continue;
                }
                app.toast(
                    Toast::new(format!("Added {}", s.front.name)).severity(Severity::Success),
                    cx,
                );
                app.skills.retain(|x| x.front.name != s.front.name);
                app.skills.push(s);
            }
            Err(e) => app.toast(
                Toast::new("Not a valid skill")
                    .body(e.to_string())
                    .severity(Severity::Danger),
                cx,
            ),
        }
    }
    cx.notify();
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafted_skills_pass_the_validator() {
        let mut trace = kai_core::Trace::new("Open bmsce.ac.in and find the IEEE CS page");
        let d = kai_core::Decision::parse_checked(
            r#"{"thought":"go","action":"open_url","text":"https://bmsce.ac.in","risk":"none"}"#,
        )
        .unwrap();
        trace.push(kai_core::trace::StepRecord {
            index: 0,
            decision: d,
            timings: Default::default(),
            backend: "x".into(),
            elements: String::new(),
            screenshot: None,
            approval: None,
            verified: true,
            output: None,
            path: None,
            call: Default::default(),
            executor: None,
        });
        let md = draft_from(&trace.task.clone(), Some(&trace));
        let name = slug(&trace.task);
        let s = kai_core::skill::load_from_str(&md, Path::new(&name)).unwrap();
        assert_eq!(s.steps.len(), 1);
        assert_eq!(s.front.name, "open-bmsce-ac-in-and-find");
    }

    #[test]
    fn frontmatter_is_left_out_of_the_body() {
        let md = "---\nname: x\ndescription: y\n---\n# Title\n\nBody\n";
        assert_eq!(body_of(md), "# Title\n\nBody\n");
        assert_eq!(body_of("# No frontmatter"), "# No frontmatter");
        assert_eq!(body_of("---\nunclosed"), "---\nunclosed");
    }

    #[test]
    fn slugs_follow_the_name_rule() {
        assert_eq!(slug("Play lofi -- hip hop!"), "play-lofi-hip-hop");
        assert_eq!(slug("!!!"), "my-skill");
        assert!(kai_core::skill::valid_name(&slug(&"word ".repeat(40))));
    }
}
