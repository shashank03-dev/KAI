//! First run, one thing per step: what Kai is, connect a model (with a real
//! test), grant access, choose how careful Kai is, then try a safe task.

use crate::AgentHandle;
use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, SegmentedControl},
    chat::{CapabilityCards, SuggestionChips},
    data_display::{Badge, Tone},
    feedback::{Callout, ErrorView, InlineMessage},
    forms::{HotkeyInput, PasswordInput, RadioCard, TextInput, UrlInput},
    onboarding::{OnboardingStep, OnboardingWizard},
    primitives::{IconName, Severity},
    theme::{ActiveTheme, TextSize},
};
use gpui::{
    AppContext, Context, Entity, IntoElement, Keystroke, ParentElement, Render, Styled, WeakEntity,
    Window, div,
};
use kai_core::risk::ApprovalLevel;
use kai_core::settings::{BackendChoice, Settings};
use kai_os::permissions::{Check, Status};
use std::time::Duration;

const DEMO: &str = "Open bmsce.ac.in and find the IEEE CS page";

pub struct Onboarding {
    app: WeakEntity<KaiApp>,
    step: usize,
    settings: Settings,
    endpoint: Entity<TextInput>,
    vllm_key: Entity<TextInput>,
    gemini_key: Entity<TextInput>,
    testing: bool,
    tested: Option<Result<(Duration, String), String>>,
    checks: Vec<Check>,
}

impl Onboarding {
    pub fn new(
        settings: Settings,
        app: WeakEntity<KaiApp>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let endpoint = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("https://<droplet>.sslip.io/v1")
                .label("Server address")
        });
        let vllm_key = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Server API key")
                .label("Server API key")
                .masked()
        });
        let gemini_key = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Gemini API key")
                .label("Gemini API key")
                .masked()
        });
        let me = Self {
            app,
            step: 0,
            settings,
            endpoint,
            vllm_key,
            gemini_key,
            testing: false,
            tested: None,
            checks: Vec::new(),
        };
        // Poll access once a second while onboarding is open, so the status turns green by itself.
        cx.spawn(async move |this, cx| {
            loop {
                let rx = match this.update(cx, |_, cx| cx.global::<AgentHandle>().0.permissions()) {
                    Ok(rx) => rx,
                    Err(_) => break,
                };
                if let Ok(checks) = rx.await
                    && this
                        .update(cx, |o, cx| {
                            o.checks = checks;
                            cx.notify();
                        })
                        .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();
        me
    }

    /// Used by the capture harness to show each step.
    pub fn set_step(&mut self, step: usize, cx: &mut Context<Self>) {
        self.step = step;
        cx.notify();
    }

    fn keys(&self, cx: &Context<Self>) -> kai_core::settings::Keys {
        let v = self.vllm_key.read(cx).text().trim().to_string();
        let g = self.gemini_key.read(cx).text().trim().to_string();
        kai_core::settings::Keys {
            vllm: (!v.is_empty()).then_some(v),
            gemini: (!g.is_empty()).then_some(g),
            ..Default::default()
        }
    }

    fn test(&mut self, cx: &mut Context<Self>) {
        self.settings.vllm_endpoint = self.endpoint.read(cx).text().trim().to_string();
        self.testing = true;
        self.tested = None;
        let rx = cx
            .global::<AgentHandle>()
            .0
            .test_connection(self.settings.clone(), self.keys(cx));
        cx.spawn(async move |this, cx| {
            let r = rx
                .await
                .unwrap_or_else(|_| Err("Kai's agent stopped.".into()));
            let _ = this.update(cx, |o, cx| {
                o.testing = false;
                o.tested = Some(r);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn finish(&mut self, demo: bool, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self.keys(cx);
        for (name, value) in [
            (kai_os::secrets::VLLM, keys.vllm),
            (kai_os::secrets::GEMINI, keys.gemini),
        ] {
            if let Some(v) = value
                && let Err(e) = kai_os::secrets::set(name, &v)
            {
                log::error!("onboarding: key not stored: {e}");
            }
        }
        self.settings.vllm_endpoint = self.endpoint.read(cx).text().trim().to_string();
        self.settings.onboarded = true;
        let settings = self.settings.clone();
        let _ = self.app.update(cx, |app, cx| {
            app.settings = settings;
            app.pages.endpoint.update(cx, |f, cx| {
                f.set_text(app.settings.vllm_endpoint.clone(), cx)
            });
            app.onboarding = None;
            app.save_settings(cx);
            app.check_connection(window, cx);
            if demo {
                app.start_run(Some(DEMO.to_string()), window, cx);
            }
        });
    }

    fn ready(&self) -> bool {
        match self.step {
            1 => matches!(self.tested, Some(Ok(_))),
            2 => !self.checks.is_empty() && kai_os::permissions::all_granted(&self.checks),
            _ => true,
        }
    }

    fn welcome(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div().flex().flex_col().gap_4()
            .child(div().text_size(cx.theme().text_size(TextSize::Md)).child("Kai looks at your screen and does a task for you, one visible step at a time. Stop it any moment."))
            .child(CapabilityCards::new("caps", [
                (IconName::Eye, "Sees your screen", "Reads buttons and fields by name."),
                (IconName::MousePointer2, "Clicks and types", "One action per step, while you watch."),
                (IconName::Shield, "Asks before risky things", "Sending, buying, deleting, passwords."),
                (IconName::Sparkles, "Runs on open Gemma 4", "Your DigitalOcean server, or the Gemini API."),
            ], |_, _, _| {}))
            .child(Callout::new(Severity::Info).title("Your screen stays yours").child("Kai sends screenshots only to the model endpoint you set. Traces stay on this computer."))
    }

    fn connect(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let pick = |choice: BackendChoice, cx: &mut Context<Self>| {
            let me = cx.entity();
            move |_: &mut Window, cx: &mut gpui::App| {
                me.update(cx, |o, cx| {
                    o.settings.backend = choice;
                    o.tested = None;
                    cx.notify();
                })
            }
        };
        let b = self.settings.backend;
        let wants_vllm = matches!(b, BackendChoice::Both | BackendChoice::Vllm);
        let wants_gemini = matches!(b, BackendChoice::Both | BackendChoice::Gemini);
        let tester = cx.entity();
        let result = match &self.tested {
            Some(Ok((took, backend))) => Some(
                InlineMessage::new(
                    Severity::Success,
                    format!(
                        "{backend} answered an image request in {} ms.",
                        took.as_millis()
                    ),
                )
                .into_any_element(),
            ),
            Some(Err(e)) => Some(
                ErrorView::new(
                    "test-error",
                    IconName::TriangleAlert,
                    "The test didn't pass",
                )
                .detail(e.clone())
                .into_any_element(),
            ),
            None => None,
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        RadioCard::new("b-both", b == BackendChoice::Both, "Both (recommended)")
                            .description(
                                "Your DigitalOcean server first, Gemini if it is down or slow.",
                            )
                            .icon(IconName::Layers)
                            .on_select(pick(BackendChoice::Both, cx)),
                    )
                    .child(
                        RadioCard::new(
                            "b-vllm",
                            b == BackendChoice::Vllm,
                            "Our DigitalOcean server",
                        )
                        .description("Gemma 4 on your own GPU Droplet via vLLM.")
                        .icon(IconName::Server)
                        .on_select(pick(BackendChoice::Vllm, cx)),
                    )
                    .child(
                        RadioCard::new("b-gemini", b == BackendChoice::Gemini, "Gemini API")
                            .description("Gemma 4 through Google's API with your key.")
                            .icon(IconName::Globe)
                            .on_select(pick(BackendChoice::Gemini, cx)),
                    ),
            )
            .children(wants_vllm.then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(UrlInput::new(&self.endpoint))
                    .child(PasswordInput::new(&self.vllm_key))
            }))
            .children(wants_gemini.then(|| PasswordInput::new(&self.gemini_key)))
            .child(
                Button::new("test", "Test with a real image")
                    .variant(ButtonVariant::Primary)
                    .loading(self.testing)
                    .on_click(move |_, _, cx| tester.update(cx, |o, cx| o.test(cx))),
            )
            .children(result)
    }

    fn access(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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

    fn safety(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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

    fn try_it(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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

impl Render for Onboarding {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let steps = [
            (
                "welcome",
                "Welcome to Kai",
                "Gemma 4's hands on your computer.",
            ),
            (
                "connect",
                "Connect the brain",
                "Where Gemma 4 runs. A real image request tests it.",
            ),
            (
                "access",
                "Let Kai see and act",
                "Each one turns green by itself once granted.",
            ),
            (
                "safety",
                "Stay in control",
                "How careful Kai is, and how to stop it.",
            ),
            ("try", "Try it", "One safe task to finish setting up."),
        ]
        .map(|(key, title, body)| OnboardingStep {
            key: key.into(),
            title: title.into(),
            body: body.into(),
        });
        let content = match self.step {
            0 => self.welcome(cx).into_any_element(),
            1 => self.connect(cx).into_any_element(),
            2 => self.access(cx).into_any_element(),
            3 => self.safety(cx).into_any_element(),
            _ => self.try_it(cx).into_any_element(),
        };
        let (walk, done, skip) = (cx.entity(), cx.entity(), cx.entity());
        div().size_full().flex().justify_center().p_8().child(
            div().w_full().max_w(gpui::rems(36.)).child(
                OnboardingWizard::new(
                    "onboarding",
                    steps,
                    self.step,
                    move |to, _, cx| {
                        walk.update(cx, |o, cx| {
                            o.step = to;
                            cx.notify();
                        })
                    },
                    move |window, cx| done.update(cx, |o, cx| o.finish(false, window, cx)),
                )
                .content(content)
                .ready(self.ready())
                .on_skip(move |window, cx| skip.update(cx, |o, cx| o.finish(false, window, cx))),
            ),
        )
    }
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
