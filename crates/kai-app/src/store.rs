//! The model store in the app: search Hugging Face and pull a model into
//! the user's catalog, with the same calls as `kai models search|pull`
//! (`docs/MODEL_STORE.md`). Network work runs on the agent's runtime. A pull
//! downloads nothing: the model gets its own card, and its Get button does
//! the download like any other model's.

use crate::AgentHandle;
use crate::app::KaiApp;
use futures::channel::oneshot;
use gpui::Context;
use kai_core::models::hub::{SearchHit, parse_ref};
use kai_core::models::{Catalog, Pulled};
use std::future::Future;

/// What the store card shows.
#[derive(Default)]
pub struct StoreState {
    pub hits: Vec<SearchHit>,
    /// "search", or the repo being pulled.
    pub busy: Option<String>,
    /// The last result: (ok, message).
    pub message: Option<(bool, String)>,
}

impl KaiApp {
    pub fn store_search(&mut self, cx: &mut Context<Self>) {
        let query = self.pages.store_query.read(cx).text().trim().to_string();
        if query.is_empty() || self.pages.store.busy.is_some() {
            return;
        }
        self.pages.store.busy = Some("search".into());
        self.pages.store.message = None;
        cx.notify();
        let work = async move { kai_os::local::hub::search(&query).await };
        self.store_job(work, cx, |app, r| match r {
            Ok(hits) if hits.is_empty() => {
                app.pages.store.hits.clear();
                app.pages.store.message =
                    Some((false, "No GGUF vision models match that name.".into()));
            }
            Ok(hits) => app.pages.store.hits = hits,
            Err(e) => app.pages.store.message = Some((false, format!("{e:#}"))),
        });
    }

    pub fn store_pull(&mut self, repo: String, cx: &mut Context<Self>) {
        if self.pages.store.busy.is_some() {
            return;
        }
        self.pages.store.busy = Some(repo.clone());
        self.pages.store.message = None;
        cx.notify();
        let work = async move {
            let e = kai_os::local::hub::resolve(&parse_ref(&repo)?).await?;
            let got = Catalog::load().add_pulled(&Catalog::user_file(), &e)?;
            anyhow::Ok((e, got))
        };
        self.store_job(work, cx, |app, r| {
            app.pages.store.message = Some(match r {
                Ok((e, Pulled::Added)) => (
                    true,
                    format!(
                        "Added {} ({:.1} GB). It is untested with Kai; its card is below.",
                        e.label, e.download_gb
                    ),
                ),
                Ok((_, Pulled::AlreadyHave(id))) => {
                    (true, format!("These weights are already here as {id}."))
                }
                Err(e) => (false, format!("{e:#}")),
            });
        });
    }

    /// Run `work` on the agent's runtime, then `done` on the UI thread and
    /// re-rank the catalog (a pull adds a card).
    fn store_job<T: Send + 'static>(
        &mut self,
        work: impl Future<Output = anyhow::Result<T>> + Send + 'static,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut KaiApp, anyhow::Result<T>) + 'static,
    ) {
        let (tx, rx) = oneshot::channel();
        cx.global::<AgentHandle>().0.runtime.spawn(async move {
            let _ = tx.send(work.await);
        });
        cx.spawn(async move |this, cx| {
            let r = rx
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("the store request was dropped")));
            let _ = this.update(cx, |app, cx| {
                app.pages.store.busy = None;
                done(app, r);
                app.local.update(cx, |l, cx| {
                    l.refresh_catalog();
                    cx.notify();
                });
                cx.notify();
            });
        })
        .detach();
    }
}
