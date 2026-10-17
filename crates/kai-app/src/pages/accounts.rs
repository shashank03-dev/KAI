//! Settings › Accounts & models: the one place to pick a provider, connect
//! it, refresh its models, and choose the model Kai uses. Choosing saves.
//! Nothing on this page sends a model request or starts a run.

use crate::app::KaiApp;
use crate::pages::connections::{self, Action, Status};
use crate::pages::settings::row;
use crate::runs::backend_label;
use crate::switcher::{BACKENDS, backend_from_key, backend_key};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    feedback::{InlineMessage, Toast},
    forms::{Choice, Input, PasswordInput, Select, UrlInput},
    primitives::Severity,
    settings::SettingsSection,
};
use gpui::{AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, div};
use kai_core::{catalog, connections::RemoteModel, settings::BackendChoice};

const CODEX_GUIDE: &str = "https://developers.openai.com/codex/quickstart";
const CLAUDE_GUIDE: &str = "https://code.claude.com/docs/en/overview";

/// Picker entries and whether the picker must be disabled. Ely's `Select`
/// requires at least one choice unless it is disabled, so an empty catalog
/// (a subscription provider before its first refresh) must disable it.
pub fn picker_state(entries: &[RemoteModel]) -> (Vec<Choice>, bool) {
    let choices = entries
        .iter()
        .map(|m| Choice::new(m.id.clone(), m.describe()))
        .collect();
    (choices, entries.is_empty())
}

fn picker(
    id: &'static str,
    label: &'static str,
    entries: &[RemoteModel],
    selected: &str,
    pick: fn(&mut KaiApp, &str, &mut Context<KaiApp>),
    cx: &Context<KaiApp>,
) -> Select {
    let (choices, empty) = picker_state(entries);
    let me = cx.entity();
    Select::new(id, choices)
        .label(label)
        .selected(selected.to_string())
        .disabled(empty)
        .placeholder(if empty {
            "No models listed yet: connect or refresh first"
        } else {
            "Choose a model"
        })
        .on_change(move |v, _, cx| me.update(cx, |a, cx| pick(a, v.as_ref(), cx)))
}

fn pick_fast(app: &mut KaiApp, id: &str, cx: &mut Context<KaiApp>) {
    if let Err(why) = connections::select_model(app, id, cx) {
        refuse(app, why, cx);
    }
}

fn pick_careful(app: &mut KaiApp, id: &str, cx: &mut Context<KaiApp>) {
    match catalog::check(app.settings.backend, id) {
        Ok(()) => {
            app.settings.careful_model = id.to_string();
            app.save_settings(cx);
            cx.notify();
        }
        Err(why) => refuse(app, why, cx),
    }
}

fn refuse(app: &KaiApp, why: String, cx: &mut Context<KaiApp>) {
    app.toast(
        Toast::new("Model not used")
            .body(why)
            .severity(Severity::Warning),
        cx,
    );
}

/// The provider and model Kai uses now, and whether they fit together.
fn current(app: &KaiApp) -> InlineMessage {
    let selection = format!("Selected: {}", crate::runs::model_label(&app.settings));
    match catalog::check_settings(&app.settings) {
        Ok(()) => InlineMessage::new(Severity::Info, format!("{selection} · saved")),
        Err(why) => InlineMessage::new(Severity::Warning, format!("{selection} · {why}")),
    }
}

fn status(app: &KaiApp) -> InlineMessage {
    let c = &app.pages.connections;
    let (severity, text) = match &c.status {
        Status::Idle => (Severity::Info, idle_hint(app.settings.backend).to_string()),
        Status::Loading(m) => (Severity::Info, m.clone()),
        Status::Connected(m) => (Severity::Success, m.clone()),
        Status::Empty(m) => (
            Severity::Warning,
            format!("{m} The provider listed no models: check the account or server."),
        ),
        Status::Disconnected(m) => (Severity::Info, m.clone()),
        Status::Canceled => (
            Severity::Warning,
            "Canceled. Nothing changed; connect again when ready.".to_string(),
        ),
        Status::Failed(e) => (Severity::Danger, e.clone()),
    };
    InlineMessage::new(severity, text)
}

fn idle_hint(backend: BackendChoice) -> &'static str {
    match backend {
        BackendChoice::Codex => {
            "Not connected yet. Connect signs in through Codex's own ChatGPT flow, then lists the models your account offers."
        }
        BackendChoice::Claude => {
            "Claude Code has no model catalog: its documented aliases are listed. Connect checks that Claude Code is signed in."
        }
        BackendChoice::Both | BackendChoice::Gemini => {
            "The Gemma models Kai documents are listed. Use Test in Model & server to check a key with one request."
        }
        BackendChoice::Local => {
            "Start a model on the Models page, then refresh to list what the local server serves."
        }
        _ => "Documented models are listed. Refresh to fetch this provider's catalog.",
    }
}

/// Can Kai fetch a catalog for this backend (or check its account)?
fn can_refresh(backend: BackendChoice) -> bool {
    !matches!(
        backend,
        BackendChoice::Both | BackendChoice::Gemini | BackendChoice::Mock
    )
}

fn link(id: &'static str, label: &'static str, url: &'static str, cx: &Context<KaiApp>) -> Button {
    let me = cx.entity();
    Button::new(id, label)
        .variant(ButtonVariant::Ghost)
        .on_click(move |_, _, cx| me.update(cx, |_, cx| cx.open_url(url)))
}

fn action(
    id: &'static str,
    label: &'static str,
    act: Action,
    app: &KaiApp,
    cx: &Context<KaiApp>,
) -> Button {
    let me = cx.entity();
    Button::new(id, label)
        .variant(ButtonVariant::Secondary)
        .disabled(app.pages.connections.busy() || app.pages.connections.missing.is_some())
        .on_click(move |_, _, cx| me.update(cx, |a, cx| connections::refresh(a, act, cx)))
}

/// The sign-in or key controls the chosen provider needs, and only those.
fn account(app: &KaiApp, cx: &Context<KaiApp>) -> AnyElement {
    let buttons = div().flex().flex_wrap().gap_2();
    match app.settings.backend {
        BackendChoice::Codex => buttons
            .child(action("connect-codex", "Connect ChatGPT / Codex", Action::Connect, app, cx))
            .child(action("logout-codex", "Sign out of Codex", Action::Disconnect, app, cx))
            .child(link("codex-guide", "Codex setup guide", CODEX_GUIDE, cx))
            .into_any_element(),
        BackendChoice::Claude => buttons
            .child(action("connect-claude", "Connect Claude Code", Action::Connect, app, cx))
            .child(action("logout-claude", "Sign out of Claude Code", Action::Disconnect, app, cx))
            .child(link("claude-guide", "Claude Code setup guide", CLAUDE_GUIDE, cx))
            .into_any_element(),
        BackendChoice::Vllm => div()
            .flex()
            .flex_col()
            .gap_2()
            .child(row("Server URL", "The API base URL ending in /v1 (DeepSeek, OpenRouter, vLLM, a loopback server…).", UrlInput::new(&app.pages.endpoint), cx))
            .child(row("API key", "Bound to this server's address. A key saved for another address is never sent here.", PasswordInput::new(&app.pages.vllm_key), cx))
            .into_any_element(),
        BackendChoice::OpenAi => row("OpenAI API key", "Stored in the OS keychain when you refresh.", PasswordInput::new(&app.pages.openai_key), cx).into_any_element(),
        BackendChoice::Gateway => row("Vercel AI Gateway key", "Stored in the OS keychain when you refresh.", PasswordInput::new(&app.pages.gateway_key), cx).into_any_element(),
        BackendChoice::Local => {
            let me = cx.entity();
            buttons
                .child(Button::new("open-models", "Open Models").variant(ButtonVariant::Secondary)
                    .on_click(move |_, _, cx| me.update(cx, |a, cx| a.go(crate::app::Page::Models, cx))))
                .into_any_element()
        }
        _ => div().into_any_element(),
    }
}

fn refresh_row(app: &KaiApp, cx: &Context<KaiApp>) -> AnyElement {
    let (refresher, canceler) = (cx.entity(), cx.entity());
    let c = &app.pages.connections;
    let refresh = Button::new("refresh-provider-models", "Refresh models")
        .loading(c.busy())
        .disabled(c.busy() || c.missing.is_some() || !can_refresh(app.settings.backend))
        .on_click(move |_, _, cx| {
            refresher.update(cx, |a, cx| connections::refresh(a, Action::Refresh, cx))
        });
    let cancel = Button::new("cancel-connection", "Cancel")
        .variant(ButtonVariant::Secondary)
        .disabled(!c.busy())
        .on_click(move |_, _, cx| {
            canceler.update(cx, |a, cx| {
                a.pages.connections.cancel();
                cx.notify();
            })
        });
    div()
        .flex()
        .flex_wrap()
        .gap_2()
        .child(refresh)
        .child(cancel)
        .into_any_element()
}

fn exact_id(app: &KaiApp, cx: &Context<KaiApp>) -> AnyElement {
    let me = cx.entity();
    let control = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(Input::new(&app.pages.model_id))
        .child(
            div().flex().child(
                Button::new("use-exact-model", "Use this exact ID")
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_, _, cx| {
                        me.update(cx, |a, cx| {
                            let id = a.pages.model_id.read(cx).text().trim().to_owned();
                            pick_fast(a, &id, cx);
                        })
                    }),
            ),
        );
    row(
        "Exact model ID",
        "For a model the catalog does not list. Sent exactly as typed; it must belong to this provider.",
        control,
        cx,
    )
    .into_any_element()
}

pub fn section(app: &KaiApp, cx: &Context<KaiApp>) -> SettingsSection {
    let provider = cx.entity();
    let providers = BACKENDS
        .iter()
        .map(|b| Choice::new(backend_key(*b), backend_label(*b)));
    let provider_picker = Select::new("catalog-provider", providers)
        .label("Provider")
        .selected(backend_key(app.settings.backend))
        .on_change(move |v, _, cx| {
            provider.update(cx, |a, cx| {
                crate::switcher::set_backend(a, backend_from_key(v.as_ref()), cx)
            })
        });
    let s = &app.settings;
    let fast = picker(
        "provider-model",
        "Model for every step",
        &connections::choices(app, &s.fast_model),
        &s.fast_model,
        pick_fast,
        cx,
    );
    let careful = picker(
        "provider-careful",
        "Careful model",
        &connections::choices(app, &s.careful_model),
        &s.careful_model,
        pick_careful,
        cx,
    );
    let missing =
        app.pages.connections.missing.clone().map(|m| {
            InlineMessage::new(Severity::Warning, SharedString::from(m)).into_any_element()
        });
    SettingsSection::new("Accounts & models")
        .description("Pick a provider, connect it, refresh its models and choose one: choosing saves it. Subscription sign-in stays with the provider and API keys stay in the OS keychain. Nothing here sends a model request.")
        .row(current(app))
        .row(row("Provider", "Switching cancels a pending request and clears the previous provider's models.", provider_picker, cx))
        .row(div().children(missing))
        .row(account(app, cx))
        .row(status(app))
        .row(refresh_row(app, cx))
        .row(row("Model for every step", "A listed model is not proof of access or screenshot support; a run is the first real request.", fast, cx))
        .row(row("Careful model", "Used for one step after a failed one.", careful, cx))
        .row(exact_id(app, cx))
        .row(prepare_gui_test(cx))
}

/// Prefill (never start) a visible-GUI-only check for the selected model.
fn prepare_gui_test(cx: &Context<KaiApp>) -> Button {
    let me = cx.entity();
    Button::new("prepare-mac-gui-test", "Prepare a GUI-only test run (does not start it)")
        .variant(ButtonVariant::Secondary)
        .on_click(move |_, _, cx| me.update(cx, |a, cx| {
            if let Err(why) = catalog::check_settings(&a.settings) {
                refuse(a, why, cx);
                return;
            }
            let s = &mut a.settings;
            s.gui_only = true;
            s.use_browser = false;
            s.team_mode = false;
            s.use_executor = false;
            s.use_procedures = false;
            s.best_of_plans = false;
            s.oai_contract = None;
            s.native_model = None;
            s.computer_tools_v1 = false;
            s.record_traces = true;
            s.careful_model = s.fast_model.clone();
            s.max_steps = 20;
            s.max_minutes = 10;
            s.approval = kai_core::risk::ApprovalLevel::HighRiskOnly;
            a.team = false;
            a.save_settings(cx);
            a.select(None, cx);
            a.draft.update(cx, |f, cx| f.set_text("Using only visible Mac GUI controls, open Calculator, calculate 17 × 23, and report the visible result. Do not open a terminal or execute code. Stop if permissions or sign-in need my help.", cx));
            a.go(crate::app::Page::Run, cx);
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_catalog_disables_the_picker() {
        let (choices, disabled) = picker_state(&[]);
        assert!(choices.is_empty() && disabled);
        let (choices, disabled) =
            picker_state(&[RemoteModel::documented("sonnet", "Claude sonnet")]);
        assert_eq!(choices.len(), 1);
        assert!(!disabled);
    }

    #[test]
    fn gemini_routes_cannot_refresh_a_catalog() {
        assert!(!can_refresh(BackendChoice::Both));
        assert!(!can_refresh(BackendChoice::Gemini));
        assert!(can_refresh(BackendChoice::Codex));
        assert!(can_refresh(BackendChoice::Vllm));
    }
}
