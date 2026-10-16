//! Onboarding's "Run on this computer" choice: the Best model for this
//! computer and the same one-tap flow as the Models page (download, start,
//! use), with its progress, Cancel and Try again.

use super::Onboarding;
use crate::local::{bytes_text, fraction, stage_text};
use crate::local_models::Fit;
use crate::pages::models_card::{fit_badge, meta};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    feedback::{Callout, ErrorView, InlineMessage},
    motion::ProgressBar,
    primitives::{IconName, Severity},
    theme::{ActiveTheme, ControlSize, TextSize},
};
use gpui::{AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, div};

impl Onboarding {
    /// The local flow finished: the server runs and Kai uses it.
    pub(super) fn local_ready(&self, cx: &gpui::App) -> bool {
        let l = self.local.read(cx);
        l.ready.is_some() && l.server.is_some()
    }

    pub(super) fn local_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let l = self.local.read(cx);
        if l.hardware.is_none() {
            let retry = self.app.clone();
            return crate::pages::models_card::hardware_probe(l.probe_failed, "ob-hw", move |cx| {
                let _ = retry.update(cx, |a, cx| a.detect_local(cx));
            });
        }
        let Some(best) = l.ranked.iter().find(|r| r.fit == Fit::Best) else {
            return Callout::new(Severity::Warning)
                .title("No open model fits this computer")
                .child("Pick the DigitalOcean server, Gemini or the AI Gateway instead.")
                .into_any_element();
        };
        let e = &best.entry;
        let app = self.app.clone();
        let action: AnyElement =
            if let Some(job) = l.job.as_ref().filter(|j| j.id == e.id) {
                let f = (!job.starting).then(|| fraction(&job.progress)).flatten();
                let canceller = app.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(theme.text_size(TextSize::Sm))
                            .child(stage_text(&job.progress, job.starting))
                            .child(div().text_color(theme.colors.fg_muted).child(
                                if job.starting {
                                    String::new()
                                } else {
                                    bytes_text(&job.progress)
                                },
                            )),
                    )
                    .child(match f {
                        Some(f) => ProgressBar::new("ob-bar", f),
                        None => ProgressBar::indeterminate("ob-bar"),
                    })
                    .child(
                        div().flex().justify_end().child(
                            Button::new("ob-cancel", "Cancel")
                                .variant(ButtonVariant::Ghost)
                                .size(ControlSize::Sm)
                                .icon(IconName::X)
                                .on_click(move |_, _, cx| {
                                    let _ = canceller.update(cx, |a, cx| a.local_cancel(cx));
                                }),
                        ),
                    )
                    .into_any_element()
            } else if let Some(f) = l.failure.as_ref().filter(|f| f.id == e.id) {
                ErrorView::new(
                    "ob-err",
                    IconName::TriangleAlert,
                    "The download didn't finish",
                )
                .body("Check your connection and free disk space, then try again.")
                .detail(f.message.clone())
                .action(
                    Button::new("ob-retry", "Try again")
                        .variant(ButtonVariant::Primary)
                        .icon(IconName::RotateCcw)
                        .on_click(move |_, _, cx| {
                            let _ = app.update(cx, |a, cx| a.local_retry(cx));
                        }),
                )
                .into_any_element()
            } else if l.ready.as_deref() == Some(e.id.as_str()) && l.server.is_some() {
                InlineMessage::new(
                    Severity::Success,
                    format!("Ready — Kai now runs on this computer with {}.", e.label),
                )
                .into_any_element()
            } else {
                let id = e.id.clone();
                Button::new("ob-get-use", "Download and use")
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Lg)
                    .icon(IconName::Download)
                    .disabled(l.job.is_some())
                    .on_click(move |_, _, cx| {
                        let _ = app.update(cx, |a, cx| a.local_get_and_use(&id, cx));
                    })
                    .into_any_element()
            };
        div()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .rounded_lg()
            .border_2()
            .border_color(theme.colors.accent)
            .bg(theme.colors.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(theme.text_size(TextSize::Lg))
                            .child(e.label.clone()),
                    )
                    .child(fit_badge(best.fit)),
            )
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Sm))
                    .text_color(theme.colors.fg_muted)
                    .child(meta(e)),
            )
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Sm))
                    .child(best.reason.clone()),
            )
            .child(div().pt_1().child(action))
            .child(
                div()
                    .text_size(theme.text_size(TextSize::Sm))
                    .text_color(theme.colors.fg_muted)
                    .child("Kai downloads only after you press the button. Other models are under Models later."),
            )
            .into_any_element()
    }
}
