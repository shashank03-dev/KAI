//! The local-models API as the app uses it: the real `kai_core::models`
//! catalog/ranking and `kai_os::local` runtime manager, behind the small
//! surface the Models page and onboarding were written against.

use std::path::PathBuf;

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

/// The built-in catalog plus the user's `models.json` override.
pub fn load_catalog() -> Catalog {
    Catalog::load()
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
