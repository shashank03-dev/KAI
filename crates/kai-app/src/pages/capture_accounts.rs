//! Screenshot fixtures for Settings › Accounts & models: each connection
//! state with made-up catalogs. Settings change only in memory, never saved;
//! no account, key or model is contacted.

use crate::app::{KaiApp, Page};
use crate::pages::connections::{ConnectionState, Status, source};
use gpui::Context;
use kai_core::catalog::{ModelOrigin, RemoteModel};
use kai_core::settings::BackendChoice;

pub const SHOTS: &[&str] = &[
    "73-accounts-saved",
    "74-accounts-codex-empty",
    "75-accounts-codex-loading",
    "76-accounts-codex-connected",
    "77-accounts-compatible-failed",
    "78-accounts-claude-canceled",
];

fn discovered(id: &str, label: &str, image_input: Option<bool>) -> RemoteModel {
    RemoteModel {
        id: id.into(),
        label: label.into(),
        origin: ModelOrigin::Discovered,
        image_input,
    }
}

/// Put the app on `backend` the way the provider picker does, minus saving.
fn on(app: &mut KaiApp, backend: BackendChoice, fast: &str) {
    app.settings.backend = backend;
    app.settings.fast_model = fast.into();
    app.settings.careful_model = fast.into();
    app.pages.connections = ConnectionState::new(backend);
}

/// Stage `name` if it is one of ours; false otherwise.
pub fn stage(app: &mut KaiApp, name: &str, cx: &mut Context<KaiApp>) -> bool {
    if !SHOTS.contains(&name) {
        return false;
    }
    match name {
        "74-accounts-codex-empty" => on(app, BackendChoice::Codex, ""),
        "75-accounts-codex-loading" => {
            on(app, BackendChoice::Codex, "");
            let src = source(&app.settings);
            app.pages.connections.begin(
                src,
                "Waiting for the provider's sign-in. Finish it in the browser window it opens.",
            );
        }
        "76-accounts-codex-connected" => {
            on(app, BackendChoice::Codex, "gpt-5.6-luna");
            let c = &mut app.pages.connections;
            c.source = source(&app.settings);
            c.models = vec![
                discovered("gpt-5.6-luna", "GPT-5.6 Luna", Some(true)),
                discovered("gpt-5.6-mini", "GPT-5.6 mini", Some(false)),
            ];
            c.status = Status::Connected(
                "Connected through Codex · plus · model access not tested".into(),
            );
        }
        "77-accounts-compatible-failed" => {
            on(app, BackendChoice::Vllm, "deepseek-chat");
            app.pages.connections.status = Status::Failed(
                "This server origin has no authorized key. Enter its API key and save it before connecting. An existing provider's key will not be reused.".into(),
            );
        }
        "78-accounts-claude-canceled" => {
            on(app, BackendChoice::Claude, "sonnet");
            app.pages.connections.status = Status::Canceled;
        }
        _ => {}
    }
    app.pages.settings_section = "connections".into();
    app.go(Page::Settings, cx);
    true
}
