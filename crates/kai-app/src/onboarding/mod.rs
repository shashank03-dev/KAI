//! First run, one thing per step: what Kai is, connect a model (with a real
//! test), grant access, choose how careful Kai is, then try a safe task.

use crate::AgentHandle;
use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    chat::CapabilityCards,
    feedback::{Callout, ErrorView, InlineMessage},
    forms::{PasswordInput, RadioCard, TextInput, UrlInput},
    onboarding::{OnboardingStep, OnboardingWizard},
    primitives::{IconName, Severity},
    theme::{ActiveTheme, TextSize},
};
use gpui::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, WeakEntity, Window,
    div,
};
use kai_core::settings::{BackendChoice, Settings};
use kai_os::permissions::Check;
use std::time::Duration;

mod local;
mod steps;

pub use steps::gh_to_gpui;

const DEMO: &str = "Open bmsce.ac.in and find the IEEE CS page";

pub struct Onboarding {
    app: WeakEntity<KaiApp>,
    step: usize,
    settings: Settings,
    endpoint: Entity<TextInput>,
    vllm_key: Entity<TextInput>,
    gemini_key: Entity<TextInput>,
    gateway_key: Entity<TextInput>,
    openai_key: Entity<TextInput>,
    testing: bool,
    tested: Option<Result<(Duration, String), String>>,
    checks: Vec<Check>,
    local: Entity<crate::local::LocalState>,
}

impl Onboarding {
    pub fn new(
        settings: Settings,
        app: WeakEntity<KaiApp>,
        local: Entity<crate::local::LocalState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&local, |_, _, cx| cx.notify()).detach();
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
        let gateway_key = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Vercel AI Gateway key")
                .label("Vercel AI Gateway key")
                .masked()
        });
        let openai_key = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("OpenAI API key")
                .label("OpenAI API key")
                .masked()
        });
        let me = Self {
            app,
            step: 0,
            settings,
            endpoint,
            vllm_key,
            gemini_key,
            gateway_key,
            openai_key,
            testing: false,
            tested: None,
            checks: Vec::new(),
            local,
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

    /// Used by the capture harness: Connect with "Run on this computer".
    pub fn show_local(&mut self, cx: &mut Context<Self>) {
        self.step = 1;
        self.settings.backend = BackendChoice::Local;
        cx.notify();
    }

    /// Used by the capture harness to show each step.
    pub fn set_step(&mut self, step: usize, cx: &mut Context<Self>) {
        self.step = step;
        cx.notify();
    }

    /// The keys typed here, falling back to any already stored, so a Test
    /// works with a key set earlier or in the environment.
    fn keys(&self, cx: &Context<Self>) -> kai_core::settings::Keys {
        let typed = |f: &Entity<TextInput>| {
            let v = f.read(cx).text().trim().to_string();
            (!v.is_empty()).then_some(v)
        };
        let stored = kai_os::secrets::keys();
        kai_core::settings::Keys {
            vllm: typed(&self.vllm_key).or(stored.vllm),
            gemini: typed(&self.gemini_key).or(stored.gemini),
            gateway: typed(&self.gateway_key).or(stored.gateway),
            openai: typed(&self.openai_key).or(stored.openai),
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
        for (name, field) in [
            (kai_os::secrets::VLLM, &self.vllm_key),
            (kai_os::secrets::GEMINI, &self.gemini_key),
            (kai_os::secrets::GATEWAY, &self.gateway_key),
            (kai_os::secrets::OPENAI, &self.openai_key),
        ] {
            let v = field.read(cx).text().trim().to_string();
            if !v.is_empty()
                && let Err(e) = kai_os::secrets::set(name, &v)
            {
                log::error!("onboarding: key not stored: {e}");
            }
        }
        self.settings.vllm_endpoint = self.endpoint.read(cx).text().trim().to_string();
        self.settings.onboarded = true;
        let mut settings = self.settings.clone();
        let _ = self.app.update(cx, |app, cx| {
            // The local flow wrote the model it set up into the app's settings.
            settings.local_model = app.settings.local_model.clone();
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

    fn ready(&self, cx: &gpui::App) -> bool {
        match self.step {
            1 if self.settings.backend == BackendChoice::Local => self.local_ready(cx),
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
                (IconName::Sparkles, "Runs on open Gemma 4", "On this computer, your own server, or an API."),
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
        let local_panel = (b == BackendChoice::Local).then(|| self.local_panel(cx));
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
                            "b-local",
                            b == BackendChoice::Local,
                            "Run on this computer — free and private",
                        )
                        .description(
                            "An open model on your own machine. No key, no bill; screenshots never leave it.",
                        )
                        .icon(IconName::Cpu)
                        .on_select(pick(BackendChoice::Local, cx)),
                    )
                    .children(local_panel)
                    .child(
                        RadioCard::new(
                            "b-vllm",
                            b == BackendChoice::Vllm,
                            "Your DigitalOcean server",
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
                    )
                    .child(
                        RadioCard::new(
                            "b-gateway",
                            b == BackendChoice::Gateway,
                            "Vercel AI Gateway",
                        )
                        .description(
                            "Gemma 4 (and GPT models for comparison) with one Gateway key.",
                        )
                        .icon(IconName::Network)
                        .on_select(pick(BackendChoice::Gateway, cx)),
                    )
                    .child(
                        RadioCard::new("b-openai", b == BackendChoice::OpenAi, "OpenAI")
                            .description(
                                "GPT models with your OpenAI key, to compare against Gemma.",
                            )
                            .icon(IconName::Sparkles)
                            .on_select(pick(BackendChoice::OpenAi, cx)),
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
            .children((b == BackendChoice::Gateway).then(|| PasswordInput::new(&self.gateway_key)))
            .children((b == BackendChoice::OpenAi).then(|| PasswordInput::new(&self.openai_key)))
            .children((b != BackendChoice::Local).then(|| {
                Button::new("test", "Test with a real image")
                    .variant(ButtonVariant::Primary)
                    .loading(self.testing)
                    .on_click(move |_, _, cx| tester.update(cx, |o, cx| o.test(cx)))
            }))
            .children(result.filter(|_| b != BackendChoice::Local))
    }
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
        // Scrolls on short windows (the Connect step is the tallest).
        ely_gpui_component::layout::ScrollArea::new("onboarding-scroll")
            .vertical()
            .child(
                div().w_full().flex().justify_center().p_8().child(
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
                        .ready(self.ready(cx))
                        .on_skip(move |window, cx| {
                            skip.update(cx, |o, cx| o.finish(false, window, cx))
                        }),
                    ),
                ),
            )
    }
}
