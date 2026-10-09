//! Onboarding steps 3 to 5 (access, safety, try it) and the hotkey format
//! helpers.

use super::{DEMO, Onboarding};
use crate::AgentHandle;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, SegmentedControl},
    chat::SuggestionChips,
    data_display::{Badge, Tone},
    feedback::Callout,
    forms::HotkeyInput,
    primitives::{IconName, Severity},
    theme::{ActiveTheme, TextSize},
};
use gpui::{Context, IntoElement, Keystroke, ParentElement, Styled, div};
use kai_core::risk::ApprovalLevel;
use kai_os::permissions::Status;

impl Onboarding {
    pub(super) fn access(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let fixer = cx.entity();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(self.checks.iter().map(|c| {
                let (label, tone) = match c.status {
                    Status::Granted => ("Granted", Tone::Success),
                    Status::Limited => ("Limited", Tone::Warning),
                    Status::Missing => ("Missing", Tone::Danger),
                };
                let fix = (c.status == Status::Missing && c.name == "Accessibility").then(|| {
                    let f = fixer.clone();
                    Button::new("fix-a11y", "Turn it on")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, _, cx| {
                            f.update(cx, |_, cx| {
                                cx.global::<AgentHandle>().0.enable_accessibility()
                            })
                        })
                });
                div()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.colors.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().child(c.name))
                            .child(Badge::new(label).tone(tone)),
                    )
                    .child(
                        div()
                            .text_size(theme.text_size(TextSize::Sm))
                            .text_color(theme.colors.fg_muted)
                            .child(c.why),
                    )
                    .children((c.status != Status::Granted).then(|| {
                        div()
                            .text_size(theme.text_size(TextSize::Sm))
                            .child(c.fix.clone())
                    }))
                    .children(fix)
            }))
            .children(
                self.checks
                    .is_empty()
                    .then(|| div().text_color(theme.colors.fg_muted).child("Checking…")),
            )
    }

    pub(super) fn safety(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (level, key) = (cx.entity(), cx.entity());
        let selected = match self.settings.approval {
            ApprovalLevel::Every => "every",
            ApprovalLevel::HighRiskOnly => "high",
            ApprovalLevel::Autopilot => "auto",
        };
        let stroke = Keystroke::parse(&gh_to_gpui(&self.settings.kill_hotkey)).ok();
        div().flex().flex_col().gap_4()
            .child(div().child("When should Kai ask you first?"))
            .child(SegmentedControl::new("approval", selected)
                .segment("every", "Every action", None)
                .segment("high", "Risky actions only", None)
                .segment("auto", "Autopilot", None)
                .on_change(move |v, _, cx| level.update(cx, |o, cx| {
                    o.settings.approval = match v.as_ref() { "every" => ApprovalLevel::Every, "auto" => ApprovalLevel::Autopilot, _ => ApprovalLevel::HighRiskOnly };
                    cx.notify();
                })))
            .child(div().text_size(cx.theme().text_size(TextSize::Sm)).text_color(cx.theme().colors.fg_muted).child("Autopilot still always asks for payments and passwords."))
            .child(div().child("Stop shortcut (works from any app)"))
            .child(HotkeyInput::new("kill", stroke).on_change(move |k, _, cx| key.update(cx, |o, cx| {
                if let Some(k) = k { o.settings.kill_hotkey = gpui_to_gh(&k); }
                cx.notify();
            })))
            .child(Callout::new(Severity::Warning).title("Three ways to stop Kai").child("Press the stop shortcut, push the mouse into the top-left corner of the screen, or press Stop in the mini window."))
    }

    pub(super) fn try_it(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let go = cx.entity();
        div().flex().flex_col().gap_4()
            .child(div().child("Try a safe task. Kai will open your browser and look around; nothing is sent or changed."))
            .child(SuggestionChips::new("demo", [(DEMO, IconName::Globe)], move |_, window, cx| go.update(cx, |o, cx| o.finish(true, window, cx))))
    }
}

/// "ctrl+alt+Escape" (global-hotkey) ⇄ "ctrl-alt-escape" (gpui).
pub fn gh_to_gpui(s: &str) -> String {
    s.split('+')
        .map(|p| p.trim().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn gpui_to_gh(k: &Keystroke) -> String {
    let mut parts = Vec::new();
    if k.modifiers.control {
        parts.push("ctrl".to_string());
    }
    if k.modifiers.alt {
        parts.push("alt".to_string());
    }
    if k.modifiers.shift {
        parts.push("shift".to_string());
    }
    if k.modifiers.platform {
        parts.push("super".to_string());
    }
    let key = match k.key.as_str() {
        "escape" => "Escape".to_string(),
        "end" => "End".to_string(),
        "space" => "Space".to_string(),
        other if other.len() == 1 => other.to_ascii_uppercase(),
        other => other.to_string(),
    };
    parts.push(key);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys_convert_both_ways() {
        assert_eq!(gh_to_gpui("ctrl+alt+Escape"), "ctrl-alt-escape");
        let k = Keystroke::parse("ctrl-alt-escape").unwrap();
        assert_eq!(gpui_to_gh(&k), "ctrl+alt+Escape");
        let k = Keystroke::parse("ctrl-shift-k").unwrap();
        assert_eq!(gpui_to_gh(&k), "ctrl+shift+K");
    }
}
