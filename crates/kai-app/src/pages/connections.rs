//! Account connections and model catalogs: one request at a time, tied to
//! the provider and endpoint it was made for. Switching provider cancels the
//! request and forgets its catalog, so a result can never land on another
//! provider. Nothing here sends an inference request.

use crate::{AgentHandle, app::KaiApp};
use futures::{StreamExt, channel::mpsc};
use gpui::Context;
use kai_core::{
    catalog,
    connections::{AccountAction, RemoteModel},
    settings::{BackendChoice, Settings},
};

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    /// Nothing fetched for this provider yet.
    #[default]
    Idle,
    Loading(String),
    Connected(String),
    /// The provider answered with no models.
    Empty(String),
    Disconnected(String),
    Canceled,
    Failed(String),
}

#[derive(Default)]
pub struct ConnectionState {
    pub status: Status,
    pub generation: u64,
    pub models: Vec<RemoteModel>,
    /// The provider and endpoint `models` came from (see [`source`]).
    pub source: String,
    pub abort: Option<tokio::task::AbortHandle>,
    /// Why the current provider cannot start here (a missing CLI).
    pub missing: Option<String>,
}

impl ConnectionState {
    pub fn new(backend: BackendChoice) -> Self {
        Self {
            missing: kai_core::connections::missing_dependency(backend),
            ..Self::default()
        }
    }

    pub fn busy(&self) -> bool {
        matches!(self.status, Status::Loading(_))
    }

    fn stop(&mut self) {
        if let Some(task) = self.abort.take() {
            task.abort();
        }
        self.generation = self.generation.wrapping_add(1);
    }

    /// A different provider: stop its request and forget its catalog.
    pub fn switch_to(&mut self, backend: BackendChoice) {
        self.stop();
        *self = Self {
            generation: self.generation,
            ..Self::new(backend)
        };
    }

    /// The person pressed Cancel: a late answer is ignored.
    pub fn cancel(&mut self) {
        if self.busy() {
            self.stop();
            self.status = Status::Canceled;
        }
    }

    /// Start a request for `source`; returns its generation.
    pub fn begin(&mut self, source: String, message: &str) -> u64 {
        self.stop();
        self.status = Status::Loading(message.into());
        self.models.clear();
        self.source = source;
        self.generation
    }

    /// Apply an answer if it still belongs to the latest request for the
    /// provider shown now. Returns whether it was applied.
    pub fn finish(
        &mut self,
        generation: u64,
        source_now: &str,
        result: Result<(String, Vec<RemoteModel>), String>,
        action: Action,
    ) -> bool {
        if generation != self.generation {
            return false;
        }
        self.abort = None;
        if source_now != self.source {
            // The endpoint changed while waiting: this catalog is for another one.
            self.status = Status::Idle;
            self.source.clear();
            return false;
        }
        self.status = match result {
            Ok((message, _)) if action == Action::Disconnect => Status::Disconnected(message),
            Ok((message, models)) if models.is_empty() => Status::Empty(message),
            Ok((message, models)) => {
                self.models = models;
                Status::Connected(message)
            }
            Err(error) => Status::Failed(error),
        };
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Refresh,
    Connect,
    Disconnect,
}

impl Action {
    fn account(self) -> AccountAction {
        match self {
            Action::Refresh => AccountAction::Refresh,
            Action::Connect => AccountAction::Connect,
            Action::Disconnect => AccountAction::Disconnect,
        }
    }
}

enum Update {
    Open(String),
    Done(Result<(String, Vec<RemoteModel>), String>),
}

/// The provider and endpoint a catalog belongs to.
pub fn source(settings: &Settings) -> String {
    format!(
        "{:?}:{}:{}",
        settings.backend, settings.vllm_endpoint, settings.local_port
    )
}

/// The models to offer for the current provider: its fetched catalog, or
/// the names Kai documents for it until one is fetched.
pub fn models(app: &KaiApp) -> Vec<RemoteModel> {
    let c = &app.pages.connections;
    if c.source == source(&app.settings) && !c.busy() {
        c.models.clone()
    } else {
        catalog::documented(app.settings.backend)
    }
}

/// Picker entries, including a hand-typed selection that fits.
pub fn choices(app: &KaiApp, selected: &str) -> Vec<RemoteModel> {
    catalog::choices(app.settings.backend, &models(app), selected)
}

/// Set the model for every step; a careful model that no longer fits the
/// provider follows it. Refused (with the reason) when the pair is invalid.
pub fn select_model(app: &mut KaiApp, id: &str, cx: &mut Context<KaiApp>) -> Result<(), String> {
    catalog::check(app.settings.backend, id)?;
    app.settings.fast_model = id.to_string();
    if catalog::check(app.settings.backend, &app.settings.careful_model).is_err() {
        app.settings.careful_model = id.to_string();
    }
    app.save_settings(cx);
    cx.notify();
    Ok(())
}

pub fn refresh(app: &mut KaiApp, action: Action, cx: &mut Context<KaiApp>) {
    if app.pages.connections.busy() || !crate::pages::model_settings::save_keys(app, cx) {
        return;
    }
    let settings = app.settings.clone();
    let source = source(&settings);
    let message = match (settings.backend, action) {
        (BackendChoice::Codex | BackendChoice::Claude, Action::Connect) => {
            "Waiting for the provider's sign-in. Finish it in the browser window it opens."
        }
        (_, Action::Disconnect) => "Signing out…",
        _ => "Fetching the account and its models…",
    };
    let generation = app.pages.connections.begin(source.clone(), message);
    let (tx, mut rx) = mpsc::unbounded();
    let task = cx.global::<AgentHandle>().0.runtime.spawn(async move {
        let result = match settings.backend {
            BackendChoice::Codex => {
                let urls = tx.clone();
                kai_core::connections::codex_account(action.account(), move |url| {
                    let _ = urls.unbounded_send(Update::Open(url));
                })
                .await
            }
            BackendChoice::Claude => kai_core::connections::claude_account(action.account()).await,
            _ => {
                let keys = crate::keys::resolve(Default::default()).await;
                kai_core::connections::discover(&settings, &keys)
                    .await
                    .map(|m| {
                        let text = format!(
                            "{} models discovered · listing does not prove account access or image support",
                            m.len()
                        );
                        (text, m)
                    })
            }
        };
        let _ = tx.unbounded_send(Update::Done(result.map_err(|e| format!("{e:#}"))));
    });
    app.pages.connections.abort = Some(task.abort_handle());
    cx.spawn(async move |this, cx| {
        while let Some(update) = rx.next().await {
            let _ = this.update(cx, |a, cx| {
                apply(a, generation, &source, action, update, cx);
                cx.notify();
            });
        }
        // The task ended without an answer (it stopped or panicked).
        let _ = this.update(cx, |a, cx| {
            let c = &mut a.pages.connections;
            if c.generation == generation && c.busy() {
                c.abort = None;
                c.status =
                    Status::Failed("The request stopped before the provider answered.".into());
                cx.notify();
            }
        });
    })
    .detach();
    cx.notify();
}

fn apply(
    app: &mut KaiApp,
    generation: u64,
    source: &str,
    action: Action,
    update: Update,
    cx: &mut Context<KaiApp>,
) {
    let c = &mut app.pages.connections;
    if c.generation != generation || c.source != source {
        return;
    }
    match update {
        Update::Open(url) => {
            cx.open_url(&url);
            c.status = Status::Loading(
                "Finish sign-in in your browser. The provider keeps your credentials; Kai never sees them."
                    .into(),
            );
        }
        Update::Done(result) => {
            let now = self::source(&app.settings);
            app.pages
                .connections
                .finish(generation, &now, result, action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> RemoteModel {
        RemoteModel::documented(id, id)
    }

    #[test]
    fn a_late_answer_for_an_old_request_is_dropped() {
        let mut c = ConnectionState::default();
        let old = c.begin("codex".into(), "…");
        let new = c.begin("codex".into(), "…");
        assert!(!c.finish(
            old,
            "codex",
            Ok(("x".into(), vec![model("a")])),
            Action::Refresh
        ));
        assert!(c.busy());
        assert!(c.finish(
            new,
            "codex",
            Ok(("ok".into(), vec![model("b")])),
            Action::Refresh
        ));
        assert_eq!(c.models, vec![model("b")]);
        assert_eq!(c.status, Status::Connected("ok".into()));
    }

    #[test]
    fn switching_provider_forgets_the_catalog_and_ignores_its_answer() {
        let mut c = ConnectionState::default();
        let g = c.begin("codex".into(), "…");
        c.switch_to(BackendChoice::OpenAi);
        assert_eq!(c.status, Status::Idle);
        assert!(c.models.is_empty() && c.source.is_empty());
        assert!(!c.finish(
            g,
            "codex",
            Ok(("x".into(), vec![model("a")])),
            Action::Refresh
        ));
        assert!(c.models.is_empty());
    }

    #[test]
    fn an_answer_for_another_endpoint_is_dropped() {
        let mut c = ConnectionState::default();
        let g = c.begin("vllm:https://a/v1".into(), "…");
        assert!(!c.finish(
            g,
            "vllm:https://b/v1",
            Ok(("x".into(), vec![model("a")])),
            Action::Refresh
        ));
        assert!(c.models.is_empty());
        assert_eq!(c.status, Status::Idle);
    }

    #[test]
    fn cancel_wins_over_a_late_answer() {
        let mut c = ConnectionState::default();
        let g = c.begin("claude".into(), "…");
        c.cancel();
        assert_eq!(c.status, Status::Canceled);
        assert!(!c.finish(
            g,
            "claude",
            Ok(("x".into(), vec![model("a")])),
            Action::Connect
        ));
        assert_eq!(c.status, Status::Canceled);
    }

    #[test]
    fn empty_failed_and_signed_out_are_distinct_states() {
        let mut c = ConnectionState::default();
        let g = c.begin("s".into(), "…");
        c.finish(g, "s", Ok(("none".into(), vec![])), Action::Refresh);
        assert_eq!(c.status, Status::Empty("none".into()));
        let g = c.begin("s".into(), "…");
        c.finish(g, "s", Err("HTTP 401".into()), Action::Refresh);
        assert_eq!(c.status, Status::Failed("HTTP 401".into()));
        let g = c.begin("s".into(), "…");
        c.finish(g, "s", Ok(("bye".into(), vec![])), Action::Disconnect);
        assert_eq!(c.status, Status::Disconnected("bye".into()));
    }
}
