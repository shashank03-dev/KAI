//! Settings › Model & server: the backend, every endpoint and key (keys live
//! in the OS keychain), a Test per backend, and the fast and careful models.

use crate::AgentHandle;
use crate::app::KaiApp;
use crate::pages::settings::row;
use crate::runs::backend_label;
use crate::switcher::{backend_from_key, backend_key};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    feedback::{ErrorView, InlineMessage, Toast},
    forms::{Choice, PasswordInput, Select, TextInput, UrlInput},
    primitives::{IconName, Severity},
    settings::SettingsSection,
};
use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div};
use kai_core::settings::BackendChoice;

/// Store any keys typed into the fields, and the endpoint. False when a key
/// couldn't be stored (a toast already says why).
pub fn save_keys(app: &mut KaiApp, cx: &mut Context<KaiApp>) -> bool {
    let mut ok = true;
    app.settings.vllm_endpoint = app.pages.endpoint.read(cx).text().trim().to_string();
    for (field, name) in [
        (app.pages.vllm_key.clone(), kai_os::secrets::VLLM),
        (app.pages.gemini_key.clone(), kai_os::secrets::GEMINI),
        (app.pages.gateway_key.clone(), kai_os::secrets::GATEWAY),
        (app.pages.openai_key.clone(), kai_os::secrets::OPENAI),
    ] {
        let v = field.read(cx).text().trim().to_string();
        if !v.is_empty() {
            match kai_os::secrets::set(name, &v) {
                Ok(()) => {
                    if name == kai_os::secrets::VLLM {
                        app.settings.compatible_key_origin =
                            kai_core::connections::endpoint_origin(&app.settings.vllm_endpoint)
                                .unwrap_or_default();
                    }
                    field.update(cx, |f, cx| f.set_text("", cx));
                }
                Err(e) => {
                    ok = false;
                    app.toast(
                        Toast::new("Key not saved")
                            .body(e.to_string())
                            .severity(Severity::Danger),
                        cx,
                    )
                }
            }
        }
    }
    app.save_settings(cx);
    ok
}

/// Send one real image request through `backend` (the chosen one when None).
pub fn test(app: &mut KaiApp, backend: Option<BackendChoice>, cx: &mut Context<KaiApp>) {
    save_keys(app, cx);
    let mut settings = app.settings.clone();
    if let Some(b) = backend {
        settings.backend = b;
    }
    let key = backend_key(settings.backend).to_string();
    app.pages.testing = key.clone();
    app.pages.test_results.retain(|(k, _)| *k != key);
    let rx = cx
        .global::<AgentHandle>()
        .0
        .test_connection(settings, Default::default());
    cx.spawn(async move |this, cx| {
        let r = rx
            .await
            .unwrap_or_else(|_| Err("Kai's agent stopped.".into()));
        let _ = this.update(cx, |a, cx| {
            a.pages.testing.clear();
            let r =
                r.map(|(d, b)| format!("{b} answered an image request in {} ms.", d.as_millis()));
            a.pages.test_results.push((key, r));
            cx.notify();
        });
    })
    .detach();
}

fn result(app: &KaiApp, key: &str) -> Option<AnyElement> {
    app.pages
        .test_results
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, r)| match r {
            Ok(m) => InlineMessage::new(Severity::Success, m.clone()).into_any_element(),
            Err(e) => ErrorView::new(
                gpui::SharedString::from(format!("test-err-{key}")),
                IconName::TriangleAlert,
                "The test didn't pass",
            )
            .detail(e.clone())
            .into_any_element(),
        })
}

/// A key field with its own Test button and result.
fn key_row(
    app: &KaiApp,
    label: &str,
    field: &Entity<TextInput>,
    backend: BackendChoice,
    cx: &Context<KaiApp>,
) -> AnyElement {
    // Rendering must not query the credential store: OS consent can block.
    let hint = "Keys stay in the OS keychain or environment. Leave blank to keep an existing key.";
    let key = backend_key(backend);
    let tester = cx.entity();
    let control = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(PasswordInput::new(field))
        .child(
            div().flex().child(
                Button::new(
                    gpui::SharedString::from(format!("test-{key}")),
                    format!("Test {}", backend_label(backend)),
                )
                .variant(ButtonVariant::Secondary)
                .loading(app.pages.testing == key)
                .on_click(move |_, _, cx| tester.update(cx, |a, cx| test(a, Some(backend), cx))),
            ),
        )
        .children(result(app, key));
    row(label, hint, control, cx).into_any_element()
}

/// The local server's status and port, and the way to the Models page.
fn local_row(app: &KaiApp, cx: &Context<KaiApp>) -> AnyElement {
    let l = app.local.read(cx);
    let status = crate::pages::models::server_text(l);
    let (open, using) = (cx.entity(), app.settings.backend == BackendChoice::Local);
    let model = if app.settings.local_model.is_empty() {
        "none chosen yet".to_string()
    } else {
        l.label(&app.settings.local_model)
    };
    let hint = format!(
        "Port {} on 127.0.0.1 · model: {model}{}",
        app.settings.local_port,
        if using { " · Kai uses it now" } else { "" }
    );
    let control = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(InlineMessage::new(
            if l.server.is_some() {
                Severity::Success
            } else {
                Severity::Info
            },
            status,
        ))
        .child(
            div().flex().child(
                Button::new("open-models", "Open Models")
                    .variant(ButtonVariant::Secondary)
                    .icon(IconName::Cpu)
                    .on_click(move |_, _, cx| {
                        open.update(cx, |a, cx| a.go(crate::app::Page::Models, cx))
                    }),
            ),
        );
    row("On this computer (local server)", &hint, control, cx).into_any_element()
}

pub fn model(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let (backend, fast, careful, saver, tester) = (
        cx.entity(),
        cx.entity(),
        cx.entity(),
        cx.entity(),
        cx.entity(),
    );
    let models = || {
        crate::pages::connections::models(app)
            .into_iter()
            .map(|(id, label)| Choice::new(id, label))
    };
    let choices = [
        BackendChoice::Both,
        BackendChoice::Vllm,
        BackendChoice::Gemini,
        BackendChoice::Gateway,
        BackendChoice::OpenAi,
        BackendChoice::Local,
        BackendChoice::Codex,
        BackendChoice::Claude,
    ]
    .into_iter()
    .map(|b| Choice::new(backend_key(b), backend_label(b)));
    let backend_picker = Select::new("backend", choices)
        .selected(backend_key(app.settings.backend))
        .on_change(move |v, window, cx| {
            backend.update(cx, |a, cx| {
                crate::switcher::set_backend(a, backend_from_key(v.as_ref()), cx);
                a.check_connection(window, cx);
            })
        });
    let tested = backend_key(app.settings.backend);
    SettingsSection::new("Model & server")
        .description("Where the model runs. Screenshots go only to these endpoints.")
        .row(row(
            "Backend",
            "Both tries your DigitalOcean server first and falls back to Gemini for a step. AI Gateway (Vercel) and OpenAI run Gemma 4 or GPT models for comparison.",
            backend_picker,
            cx,
        ))
        .row(local_row(app, cx))
        .row(row(
            "Compatible server / DigitalOcean",
            "OpenAI-compatible API URL ending in /v1. Use HTTPS for cloud providers or HTTP for a loopback server.",
            UrlInput::new(&app.pages.endpoint),
            cx,
        ))
        .row(key_row(app, "Server API key", &app.pages.vllm_key, BackendChoice::Vllm, cx))
        .row(key_row(app, "Gemini API key", &app.pages.gemini_key, BackendChoice::Gemini, cx))
        .row(key_row(app, "Vercel AI Gateway key", &app.pages.gateway_key, BackendChoice::Gateway, cx))
        .row(key_row(app, "OpenAI API key", &app.pages.openai_key, BackendChoice::OpenAi, cx))
        .row(row(
            "Fast model",
            "Used for every step. Gemma runs on every backend; GPT models need AI Gateway or OpenAI.",
            Select::new("fast", models())
                .selected(app.settings.fast_model.clone())
                .on_change(move |v, _, cx| {
                    fast.update(cx, |a, cx| crate::switcher::set_model(a, v.as_ref(), cx))
                }),
            cx,
        ))
        .row(row(
            "Careful model",
            "Used for one step after a failed one, then Kai drops back.",
            Select::new("careful", models())
                .selected(app.settings.careful_model.clone())
                .on_change(move |v, _, cx| {
                    careful.update(cx, |a, cx| {
                        a.settings.careful_model = v.to_string();
                        a.save_settings(cx);
                    })
                }),
            cx,
        ))
        .row(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("save-model", "Save")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, _, cx| {
                            saver.update(cx, |a, cx| {
                                if !save_keys(a, cx) {
                                    return;
                                }
                                a.toast(
                                    Toast::new("Saved")
                                        .body("Keys are in the OS keychain; the server address is in Kai's settings.")
                                        .severity(Severity::Success),
                                    cx,
                                );
                            })
                        }),
                )
                .child(
                    Button::new("test-model", "Save and test the chosen backend")
                        .variant(ButtonVariant::Primary)
                        .loading(app.pages.testing == tested)
                        .on_click(move |_, _, cx| tester.update(cx, |a, cx| test(a, None, cx))),
                ),
        )
        // Backends without a key row of their own show the result here.
        .row(div().children(
            matches!(app.settings.backend, BackendChoice::Both | BackendChoice::Local)
                .then(|| result(app, tested))
                .flatten(),
        ))
}
