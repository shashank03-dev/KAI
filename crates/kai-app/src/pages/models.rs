//! Models: run an open model on this computer. What the computer has, every
//! catalog model ranked for it with the reason in plain words, and the one
//! tap that downloads, starts and uses the best one.

use crate::app::{KaiApp, Page as AppPage};
use crate::local::LocalState;
use crate::local_api::{Fit, HardwareSummary};
use crate::pages::{models_card, models_store};
use crate::ui;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    feedback::Callout,
    layout::{Card, CardHeader, ScrollArea},
    motion::Spinner,
    overlays::ConfirmDialog,
    primitives::{Icon, IconName, Severity},
    theme::{ActiveTheme, ControlSize, TextSize},
};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, Window, div};
use kai_core::settings::BackendChoice;

/// "Linux x86_64" from `os`/`arch` as the hardware probe reports them.
pub fn os_text(hw: &HardwareSummary) -> String {
    let os = match hw.os.to_ascii_lowercase().as_str() {
        "linux" => "Linux".to_string(),
        "windows" => "Windows".to_string(),
        _ => hw.os.clone(),
    };
    format!("{os} {}", hw.arch)
}

fn fact(icon: IconName, label: &str, value: String, cx: &Context<KaiApp>) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .items_start()
        .gap_3()
        .min_w_0()
        .child(Icon::new(icon).color(theme.colors.fg_muted))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(theme.text_size(TextSize::Sm))
                        .text_color(theme.colors.fg_muted)
                        .child(label.to_string()),
                )
                .child(div().child(value)),
        )
}

/// "Running Gemma 4 E4B at http://127.0.0.1:8098/v1" or "Off".
pub fn server_text(l: &LocalState) -> String {
    match &l.server {
        Some((id, s)) => format!("Running {} at {}", l.label(id), s.base_url),
        None => "Off. Kai starts it when you use a model.".into(),
    }
}

fn hardware(l: &LocalState, cx: &Context<KaiApp>) -> AnyElement {
    let me = cx.entity();
    let stop = l.server.is_some().then(|| {
        Button::new("server-stop", "Stop the server")
            .variant(ButtonVariant::Outline)
            .size(ControlSize::Sm)
            .icon(IconName::Square)
            .disabled(l.job.is_some())
            .on_click(move |_, _, cx| me.update(cx, |a, cx| a.local_stop(cx)))
    });
    let mut header =
        CardHeader::new("This computer").description(format!("Local server: {}", server_text(l)));
    if let Some(s) = stop {
        header = header.action(s);
    }
    let body = match &l.hardware {
        None => div()
            .flex()
            .items_center()
            .gap_2()
            .child(Spinner::new("hw-spin"))
            .child("Looking at this computer's memory and graphics…")
            .into_any_element(),
        Some(hw) => div()
            .grid()
            .grid_cols(4)
            .gap_4()
            .child(fact(
                IconName::MemoryStick,
                "Memory",
                format!("{:.0} GB RAM", hw.ram_gb),
                cx,
            ))
            .child(fact(
                IconName::Cpu,
                "Processor",
                format!("{} threads", hw.cpu_threads),
                cx,
            ))
            .child(fact(
                IconName::Monitor,
                &match hw.gpu_vram_gb {
                    Some(v) => format!("Graphics · {v:.0} GB"),
                    None => "Graphics".into(),
                },
                hw.gpu_name
                    .clone()
                    .unwrap_or_else(|| "No graphics card found".into()),
                cx,
            ))
            .child(fact(IconName::HardDrive, "System", os_text(hw), cx))
            .into_any_element(),
    };
    Card::new().header(header).child(body).into_any_element()
}

fn ready(l: &LocalState, cx: &Context<KaiApp>) -> Option<AnyElement> {
    let id = l.ready.as_ref()?;
    let base = l
        .server
        .as_ref()
        .map(|(_, s)| s.base_url.clone())
        .unwrap_or_default();
    let me = cx.entity();
    Some(
        Callout::new(Severity::Success)
            .title("Ready — Kai now runs on this computer")
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(format!(
                        "{} answers at {base}. Screenshots stay on this computer, and there is nothing to pay.",
                        l.label(id)
                    ))
                    .child(
                        div().flex().child(
                            Button::new("ready-try", "Give Kai a task")
                                .variant(ButtonVariant::Primary)
                                .icon(IconName::Play)
                                .on_click(move |_, _, cx| {
                                    me.update(cx, |a, cx| {
                                        a.select(None, cx);
                                        a.go(AppPage::Run, cx)
                                    })
                                }),
                        ),
                    ),
            )
            .into_any_element(),
    )
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let l = app.local.read(cx);
    let in_use =
        (app.settings.backend == BackendChoice::Local).then(|| app.settings.local_model.clone());
    let none_fit = l.hardware.is_some() && l.ranked.iter().all(|r| r.fit == Fit::TooBig);
    let cards = l.ranked.iter().enumerate().map(|(ix, r)| {
        let used = in_use.as_deref() == Some(r.entry.id.as_str());
        crate::anim::enter(
            div().child(models_card::card(r, l, used, cx)),
            ("model-in", ix),
            ix,
        )
    });
    let store = models_store::card(app, cx);
    let theme = cx.theme();
    let note = div()
        .text_size(theme.text_size(TextSize::Sm))
        .text_color(theme.colors.fg_muted)
        .child(format!(
            "Kai downloads only when you press a button. The runtime is llama.cpp; models and runtime are kept in {}. The server listens on 127.0.0.1 only.",
            l.manager.root.display()
        ));
    let confirm = app.pages.confirm_remove_model.clone().map(|id| {
        let (closer, doer) = (cx.entity(), cx.entity());
        let size = l
            .ranked
            .iter()
            .find(|r| r.entry.id == id)
            .map(|r| format!(" and frees about {:.1} GB", r.entry.download_gb))
            .unwrap_or_default();
        ConfirmDialog::new(
            "confirm-remove-model",
            format!("Remove {}?", l.label(&id)),
            format!("This deletes the downloaded model from this computer{size}. You can download it again later."),
            move |_, cx| {
                closer.update(cx, |a, cx| {
                    a.pages.confirm_remove_model = None;
                    cx.notify();
                })
            },
        )
        .confirm("Remove")
        .destructive()
        .on_confirm(move |_, cx| {
            doer.update(cx, |a, cx| {
                a.pages.confirm_remove_model = None;
                a.local_remove(&id, cx);
            })
        })
    });
    let page = ScrollArea::new("models").vertical().child(ui::body(
        1040.,
        ui::page(
            "On this computer",
            "Models",
            "Run an open model on this computer: free, private, no key. Your screen never leaves this machine.",
            None,
            cx,
        )
            .child(hardware(l, cx))
            .children(ready(l, cx))
            .children(none_fit.then(|| {
                Callout::new(Severity::Warning)
                    .title("None of these models fit this computer")
                    .child("Use the DigitalOcean server, the Gemini API or the AI Gateway instead (Settings › Model & server).")
            }))
            .child(div().flex().flex_col().gap_3().children(cards))
            .child(store)
            .child(note),
    ));
    div().size_full().child(page).children(confirm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_reads_like_a_name() {
        let hw = HardwareSummary {
            ram_gb: 8.0,
            cpu_threads: 4,
            gpu_vram_gb: None,
            gpu_name: None,
            os: "windows".into(),
            arch: "x86_64".into(),
        };
        assert_eq!(os_text(&hw), "Windows x86_64");
    }
}
