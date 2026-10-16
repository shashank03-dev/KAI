//! A skill's details sheet: its files, whether it is valid, and its
//! instructions, read from disk once when the sheet opens (not on every
//! repaint). A skill of your own can be deleted from here.

use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    documents::MarkdownRenderer,
    feedback::{InlineMessage, Toast},
    layout::{ScrollArea, Sheet},
    overlays::ConfirmDialog,
    primitives::{IconName, Place, Severity},
    theme::{ActiveTheme, TextSize},
};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div};
use kai_core::Skill;
use kai_core::settings::Settings;
use std::path::Path;

/// What the sheet shows, read when it opens.
pub struct Detail {
    pub files: Vec<String>,
    /// Valid against the Agent Skill Open Standard, or why not.
    pub valid: Result<(), String>,
    /// SKILL.md without its frontmatter.
    pub body: String,
}

impl Detail {
    pub fn load(s: &Skill) -> Self {
        let mut files: Vec<String> = vec!["SKILL.md".into()];
        for sub in ["scripts", "references", "assets"] {
            if let Ok(rd) = std::fs::read_dir(s.dir.join(sub)) {
                files.extend(
                    rd.flatten()
                        .map(|e| format!("{sub}/{}", e.file_name().to_string_lossy())),
                );
            }
        }
        let md = std::fs::read_to_string(s.dir.join("SKILL.md")).unwrap_or_default();
        Self {
            files,
            valid: kai_core::skill::load(&s.dir)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            // The frontmatter is already shown above (name, description);
            // left in, Markdown reads its closing `---` as a heading underline.
            body: super::skills::body_of(&md).to_string(),
        }
    }
}

pub fn sheet(s: &Skill, d: &Detail, cx: &mut Context<KaiApp>) -> AnyElement {
    let (closer, asker) = (cx.entity(), cx.entity());
    let theme = cx.theme();
    let file_list = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(theme.text_size(TextSize::Sm))
                .text_color(theme.colors.fg_muted)
                .child(format!("Files ({})", d.files.len())),
        )
        .children(d.files.iter().map(|f| {
            div()
                .text_size(theme.text_size(TextSize::Sm))
                .font_family(theme.mono_family.clone())
                .child(f.clone())
        }));
    let description = div()
        .text_size(theme.text_size(TextSize::Sm))
        .text_color(theme.colors.fg_muted)
        .child(s.front.description.clone());
    let valid = match &d.valid {
        Ok(()) => InlineMessage::new(
            Severity::Success,
            "Valid against the Agent Skill Open Standard.",
        ),
        Err(e) => InlineMessage::new(Severity::Danger, e.clone()),
    };
    // Built-in skills ship with Kai; switch one off on its card instead.
    let delete = (!s.builtin).then(|| {
        Button::new("skill-delete", "Delete skill")
            .variant(ButtonVariant::Danger)
            .icon(IconName::Trash2)
            .on_click(move |_, _, cx| {
                asker.update(cx, |a, cx| {
                    a.pages.confirm_delete_skill = true;
                    cx.notify();
                })
            })
    });
    Sheet::new(
        "skill-sheet",
        Place::Right,
        s.front.name.clone(),
        move |_, cx| {
            closer.update(cx, |a, cx| {
                a.pages.open_skill(None);
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
                .child(MarkdownRenderer::new("skill-md", d.body.clone()))
                .children(delete.map(|b| div().pt_2().child(b))),
        ),
    )
    .into_any_element()
}

/// "Delete <skill>?", while the user decides.
pub fn confirm_delete(app: &KaiApp, cx: &mut Context<KaiApp>) -> Option<AnyElement> {
    if !app.pages.confirm_delete_skill {
        return None;
    }
    let name = app.pages.open_skill.clone()?;
    let (closer, doer) = (cx.entity(), cx.entity());
    let dialog = ConfirmDialog::new(
        "confirm-delete-skill",
        format!("Delete {name}?"),
        "Its folder is removed from this computer. To keep it but stop Kai using it, switch it off instead. This can't be undone.",
        move |_, cx| {
            closer.update(cx, |a, cx| {
                a.pages.confirm_delete_skill = false;
                cx.notify();
            })
        },
    )
    .confirm("Delete")
    .destructive()
    .on_confirm(move |_, cx| doer.update(cx, |a, cx| delete(a, &name, cx)));
    Some(dialog.into_any_element())
}

fn delete(app: &mut KaiApp, name: &str, cx: &mut Context<KaiApp>) {
    app.pages.confirm_delete_skill = false;
    let Some(s) = app.skills.iter().find(|s| s.front.name == name) else {
        return;
    };
    let result = if s.builtin || !deletable(&s.dir, &Settings::skills_dir()) {
        Err(std::io::Error::other(
            "only skills in your skills folder can be deleted",
        ))
    } else {
        std::fs::remove_dir_all(&s.dir)
    };
    let toast = match result {
        Ok(()) => {
            app.skills.retain(|s| s.front.name != name);
            app.pages.disabled.remove(name);
            app.settings.disabled_skills = app.pages.disabled.iter().cloned().collect();
            app.save_settings(cx);
            app.pages.open_skill(None);
            Toast::new(format!("Deleted {name}")).severity(Severity::Success)
        }
        Err(e) => Toast::new("Skill not deleted")
            .body(e.to_string())
            .severity(Severity::Danger),
    };
    app.toast(toast, cx);
    cx.notify();
}

/// Only a folder inside your skills folder is ever deleted: never the
/// skills folder itself, a built-in skill, or a path that leads elsewhere.
fn deletable(dir: &Path, root: &Path) -> bool {
    match (dir.canonicalize(), root.canonicalize()) {
        (Ok(dir), Ok(root)) => dir != root && dir.starts_with(&root),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::deletable;

    #[test]
    fn only_folders_inside_the_skills_folder_are_deletable() {
        let base = std::env::temp_dir().join(format!("kai-skill-del-{}", std::process::id()));
        let (root, other) = (base.join("skills"), base.join("elsewhere"));
        std::fs::create_dir_all(root.join("mine")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        assert!(deletable(&root.join("mine"), &root));
        assert!(!deletable(&root, &root), "the skills folder itself");
        assert!(!deletable(&other, &root));
        assert!(!deletable(&root.join("mine/../../elsewhere"), &root));
        assert!(!deletable(&root.join("missing"), &root));
        std::fs::remove_dir_all(base).unwrap();
    }
}
