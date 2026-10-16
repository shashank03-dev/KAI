//! Local open models as the app uses them: the real `kai_core::models`
//! catalog/ranking and `kai_os::local` runtime manager, behind the small
//! surface the Models page and onboarding were written against. In-process
//! only: nothing here serves or listens (the model server binds loopback,
//! in `kai_os::local`).

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub use kai_core::models::{Catalog, Fit, HardwareSummary, ModelEntry, Ranked, rank};

/// Download progress for the UI.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    /// "runtime", "model", "verify" or "done".
    pub stage: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub message: String,
}

/// A running local model server (Kai keeps it alive until Stop or exit).
#[derive(Debug, Clone, PartialEq)]
pub struct LocalServer {
    pub base_url: String,
    pub pid: u32,
}

static CATALOG: RwLock<Option<Arc<Catalog>>> = RwLock::new(None);

/// The built-in catalog plus the user's `models.json` override, read from
/// disk once: labels are looked up while the status bar renders.
pub fn load_catalog() -> Arc<Catalog> {
    if let Some(c) = CATALOG.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return c.clone();
    }
    reload_catalog()
}

/// Read the catalog again (a model was pulled from the Hub).
pub fn reload_catalog() -> Arc<Catalog> {
    let fresh = Arc::new(Catalog::load());
    *CATALOG.write().unwrap_or_else(|e| e.into_inner()) = Some(fresh.clone());
    fresh
}

/// A catalog id's label ("Gemma 4 E4B"), if the catalog lists it.
pub fn catalog_label(id: &str) -> Option<String> {
    load_catalog()
        .models
        .iter()
        .find(|m| m.id == id)
        .map(|m| m.label.clone())
}

pub fn detect_hardware() -> HardwareSummary {
    kai_os::local::detect_hardware()
}

/// Where local runtimes and models live.
pub fn local_root() -> PathBuf {
    kai_os::local::LocalManager::default_root()
}

/// Thin wrapper over [`kai_os::local::LocalManager`].
pub struct LocalManager {
    pub root: PathBuf,
    inner: kai_os::local::LocalManager,
}

impl LocalManager {
    pub fn new(root: PathBuf) -> Self {
        Self {
            inner: kai_os::local::LocalManager::new(root.clone()),
            root,
        }
    }

    /// Download the runtime and the model files (resumable; verified).
    pub async fn install(
        &self,
        entry: &ModelEntry,
        progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> anyhow::Result<()> {
        let relay = |p: kai_os::local::Progress| {
            progress(Progress {
                stage: p.stage.to_string(),
                bytes: p.bytes,
                total: p.total,
                message: p.message,
            })
        };
        self.inner.install(entry, &relay).await
    }

    /// Stop an install in progress; partial files are kept for resuming.
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    pub fn installed(&self) -> Vec<String> {
        self.inner.installed()
    }

    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        self.inner.remove(id)
    }

    /// Start the model's server and keep it running after this call.
    pub async fn start(&self, entry: &ModelEntry, port: u16) -> anyhow::Result<LocalServer> {
        let server = self.inner.start(entry, port).await?;
        let out = LocalServer {
            base_url: server.base_url.clone(),
            pid: server.pid,
        };
        server.detach();
        Ok(out)
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        self.inner.stop().map(|_| ())
    }

    pub fn status(&self) -> Option<LocalServer> {
        self.inner.running().map(|i| LocalServer {
            base_url: i.base_url,
            pid: i.pid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_read_once_until_reloaded() {
        let first = load_catalog();
        assert!(Arc::ptr_eq(&first, &load_catalog()));
        let fresh = reload_catalog();
        assert!(Arc::ptr_eq(&fresh, &load_catalog()));
        let id = &fresh.models[0].id;
        assert_eq!(catalog_label(id).as_ref(), Some(&fresh.models[0].label));
        assert_eq!(catalog_label("no-such-model"), None);
    }
}
