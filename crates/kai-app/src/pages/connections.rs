//! Settings account connections and the shared cloud model catalog.
use crate::{AgentHandle, app::KaiApp};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    feedback::InlineMessage,
    forms::{Choice, Input, PasswordInput, Select, UrlInput},
    primitives::Severity,
    settings::SettingsSection,
};
use futures::{StreamExt, channel::mpsc};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div};
use kai_core::{
    connections::{AccountAction, RemoteModel},
    settings::BackendChoice,
};

#[derive(Default)]
pub struct ConnectionState {
    pub busy: bool,
    pub generation: u64,
    pub message: String,
    pub error: bool,
    pub models: Vec<RemoteModel>,
    pub source: String,
    pub abort: Option<tokio::task::AbortHandle>,
}

enum Update {
    Open(String),
    Done(Result<(String, Vec<RemoteModel>), String>),
}

pub fn source(app: &KaiApp) -> String {
    format!(
        "{:?}:{}:{}",
        app.settings.backend, app.settings.vllm_endpoint, app.settings.local_port
    )
}

pub fn models(app: &KaiApp) -> Vec<(String, String)> {
    if app.pages.connections.source == source(app) {
        app.pages
            .connections
            .models
            .iter()
            .map(|m| (m.id.clone(), m.label.clone()))
            .collect()
    } else if matches!(
        app.settings.backend,
        BackendChoice::Codex | BackendChoice::Claude
    ) {
        vec![]
    } else {
        kai_core::settings::MODELS
            .iter()
            .map(|m| (m.id.into(), m.label.into()))
            .collect()
    }
}

pub fn refresh(app: &mut KaiApp, action: AccountAction, cx: &mut Context<KaiApp>) {
    if app.pages.connections.busy {
        return;
    }
    if !crate::pages::model_settings::save_keys(app, cx) {
        return;
    }
    app.pages.connections.generation = app.pages.connections.generation.wrapping_add(1);
    let generation = app.pages.connections.generation;
    let settings = app.settings.clone();
    let source = source(app);
    app.pages.connections.busy = true;
    app.pages.connections.error = false;
    app.pages.connections.message =
        "Connecting to provider… Complete any sign-in window opened by the provider.".into();
    app.pages.connections.models.clear();
    app.pages.connections.source = source.clone();
    let (tx, mut rx) = mpsc::unbounded();
    let task = cx.global::<AgentHandle>().0.runtime.spawn(async move {
        let result = if settings.backend == BackendChoice::Codex {
            let urls = tx.clone();
            kai_core::connections::codex_account(action, move |url| {
                let _ = urls.unbounded_send(Update::Open(url));
            })
            .await
        } else if settings.backend == BackendChoice::Claude {
            kai_core::connections::claude_account(action).await
        } else {
            let keys = crate::keys::resolve(Default::default()).await;
            kai_core::connections::discover(&settings, &keys)
                .await
                .map(|m| {
                    (
                        format!(
                            "{} models discovered · vision and account access not tested",
                            m.len()
                        ),
                        m,
                    )
                })
        };
        let _ = tx.unbounded_send(Update::Done(result.map_err(|e| format!("{e:#}"))));
    });
    app.pages.connections.abort = Some(task.abort_handle());
    cx.spawn(async move |this, cx| {
        while let Some(update) = rx.next().await {
            let _ = this.update(cx, |a, cx| {
                if a.pages.connections.generation != generation { return; }
                match update {
                    Update::Open(url) => {
                        cx.open_url(&url);
                        a.pages.connections.message = "Complete sign-in in your browser. Kai does not receive your credentials.".into();
                    }
                    Update::Done(result) => {
                        let c = &mut a.pages.connections;
                        c.busy = false;
                        c.abort = None;
                        if source == self::source(a) {
                            let c = &mut a.pages.connections;
                            match result {
                                Ok((message, models)) => {
                                    c.message = message; c.models = models; c.error = false;
                                    if a.settings.backend == BackendChoice::Codex
                                        && !c.models.iter().any(|m| m.id == a.settings.fast_model)
                                        && c.models.iter().any(|m| m.id == "gpt-5.6-luna") {
                                        a.settings.fast_model = "gpt-5.6-luna".into();
                                        a.settings.careful_model = "gpt-5.6-luna".into();
                                        a.save_settings(cx);
                                    }
                                }
                                Err(error) => { c.message = error; c.models.clear(); c.error = true; }
                            }
                        }
                    }
                }
                cx.notify();
            });
        }
        let _ = this.update(cx, |a, cx| {
            if a.pages.connections.generation == generation && a.pages.connections.busy {
                a.pages.connections.busy = false;
                a.pages.connections.abort = None;
                a.pages.connections.message = "Connection canceled. Reconnect when ready.".into();
            }
            cx.notify();
        });
    }).detach();
    cx.notify();
}

pub fn catalog(app: &KaiApp, cx: &Context<KaiApp>) -> AnyElement {
    let (refresh_me, use_me, exact_me) = (cx.entity(), cx.entity(), cx.entity());
    let choices = models(app)
        .into_iter()
        .map(|(id, label)| Choice::new(id, label));
    div().flex().flex_col().gap_2()
        .child(InlineMessage::new(if app.pages.connections.error { Severity::Warning } else { Severity::Info },
            if app.pages.connections.message.is_empty() { "Connect or refresh to fetch the selected provider's catalog. Listing does not verify vision support or model access.".into() } else { app.pages.connections.message.clone() }))
        .child(Button::new("refresh-provider-models", "Refresh account and models").loading(app.pages.connections.busy)
            .variant(ButtonVariant::Secondary).on_click(move |_,_,cx| refresh_me.update(cx,|a,cx| refresh(a, AccountAction::Refresh,cx))))
        .child(Select::new("provider-model", choices).selected(app.settings.fast_model.clone()).on_change(move |id,_,cx| use_me.update(cx,|a,cx| {
            a.settings.fast_model = id.to_string();
            // Provider-specific models must not escalate into an unrelated static model.
            a.settings.careful_model = id.to_string();
            a.save_settings(cx);
        })))
        .child(Input::new(&app.pages.model_id))
        .child(Button::new("use-exact-model", "Use this model ID").variant(ButtonVariant::Secondary).on_click(move |_,_,cx| exact_me.update(cx,|a,cx| {
            let id = a.pages.model_id.read(cx).text().trim().to_owned();
            if !id.is_empty() {
                a.settings.fast_model = id.clone(); a.settings.careful_model = id; a.save_settings(cx);
            }
        })))
        .into_any_element()
}

pub fn section(app: &KaiApp, cx: &Context<KaiApp>) -> SettingsSection {
    let (connect, logout, cancel, help, preset) = (
        cx.entity(),
        cx.entity(),
        cx.entity(),
        cx.entity(),
        cx.entity(),
    );
    let provider = cx.entity();
    let (claude, claude_help, claude_logout) = (cx.entity(), cx.entity(), cx.entity());
    SettingsSection::new("Connected accounts & model catalog")
        .description("Subscription sign-in stays with the provider. API keys stay in your OS keychain. Kai's run view shows every requested action.")
        .row(div().flex().gap_2()
            .child(Button::new("connect-codex", "Connect ChatGPT / Codex").loading(app.pages.connections.busy).on_click(move |_,_,cx| connect.update(cx,|a,cx| {
                if a.pages.connections.busy { return; }
                crate::switcher::set_backend(a, BackendChoice::Codex, cx);
                refresh(a,AccountAction::Connect,cx);
            })))
            .child(Button::new("codex-setup-help", "Install / setup Codex").variant(ButtonVariant::Secondary).on_click(move |_,_,cx| help.update(cx,|_,cx| cx.open_url("https://developers.openai.com/codex/quickstart"))))
            .child(Button::new("cancel-connection", "Cancel connection").variant(ButtonVariant::Secondary).disabled(!app.pages.connections.busy).on_click(move |_,_,cx| cancel.update(cx,|a,_cx| {
                if let Some(task) = a.pages.connections.abort.take() { task.abort(); }
            }))))
        .row(Button::new("logout-codex", "Sign out of Codex on this computer").variant(ButtonVariant::Secondary).disabled(app.pages.connections.busy).on_click(move |_,_,cx| logout.update(cx,|a,cx| {
            crate::switcher::set_backend(a, BackendChoice::Codex, cx); refresh(a, AccountAction::Disconnect,cx);
        })))
        .row(div().flex().gap_2()
            .child(Button::new("connect-claude", "Connect Claude Code").disabled(app.pages.connections.busy).on_click(move |_,_,cx| claude.update(cx,|a,cx| {
                crate::switcher::set_backend(a, BackendChoice::Claude, cx); refresh(a,AccountAction::Connect,cx);
            })))
            .child(Button::new("claude-setup", "Install / setup Claude Code").variant(ButtonVariant::Secondary).on_click(move |_,_,cx| claude_help.update(cx,|_,cx| cx.open_url("https://code.claude.com/docs/en/overview"))))
            .child(Button::new("claude-logout", "Sign out of Claude Code on this computer").variant(ButtonVariant::Secondary).disabled(app.pages.connections.busy).on_click(move |_,_,cx| claude_logout.update(cx,|a,cx| {
                crate::switcher::set_backend(a, BackendChoice::Claude, cx); refresh(a,AccountAction::Disconnect,cx);
            }))))
        .row(crate::pages::settings::row("Provider", "Choose the account or endpoint whose models you want to fetch.",
            Select::new("catalog-provider", crate::switcher::BACKENDS.iter().filter(|b| !matches!(b, BackendChoice::Both | BackendChoice::Gemini)).map(|b| Choice::new(crate::switcher::backend_key(*b), crate::runs::backend_label(*b))))
                .selected(crate::switcher::backend_key(app.settings.backend))
                .on_change(move |v,_,cx| provider.update(cx,|a,cx| crate::switcher::set_backend(a,crate::switcher::backend_from_key(v.as_ref()),cx))),cx))
        .row(crate::pages::settings::row("Compatible server URL", "For a custom provider: its API base URL ending in /v1. Refresh saves this address.", UrlInput::new(&app.pages.endpoint),cx))
        .row(crate::pages::settings::row("Compatible server API key", "Optional for a local server. Refresh stores a supplied key in the OS keychain.", PasswordInput::new(&app.pages.vllm_key),cx))
        .row(catalog(app,cx))
        .row(InlineMessage::new(Severity::Info,"For DeepSeek, OpenRouter or another compatible service, select Compatible server and enter its /v1 URL and key above. Local servers can use an empty key. Choose a vision model for computer use."))
        .row(Button::new("prepare-mac-gui-test", "Prepare Mac GUI testing with selected model").variant(ButtonVariant::Primary).on_click(move |_,_,cx| preset.update(cx,|a,cx| {
            if matches!(a.settings.backend, BackendChoice::Codex | BackendChoice::Claude)
                && !models(a).iter().any(|(id,_)| id == &a.settings.fast_model) {
                a.pages.connections.error = true;
                a.pages.connections.message = "Refresh the account and select a model from its catalog before preparing GUI testing.".into();
                cx.notify();
                return;
            }
            a.settings.gui_only = true;
            a.settings.use_browser = false;
            a.settings.team_mode = false; a.team = false;
            a.settings.use_executor = false;
            a.settings.use_procedures = false;
            a.settings.best_of_plans = false;
            a.settings.oai_contract = None;
            a.settings.native_model = None;
            a.settings.computer_tools_v1 = false;
            a.settings.record_traces = true;
            a.settings.careful_model = a.settings.fast_model.clone();
            a.settings.max_steps = 20; a.settings.max_minutes = 10;
            a.settings.approval = kai_core::risk::ApprovalLevel::HighRiskOnly;
            a.save_settings(cx);
            a.select(None,cx);
            a.draft.update(cx,|f,cx| f.set_text("Using only visible Mac GUI controls, open Calculator, calculate 17 × 23, and report the visible result. Do not open a terminal or execute code. Stop if permissions or sign-in need my help.",cx));
            a.go(crate::app::Page::Run,cx);
        })))
}
