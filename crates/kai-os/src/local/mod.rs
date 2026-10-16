//! Open models on this computer: hardware detection, downloading the
//! llama.cpp runtime and model files (resumable, verified), and running
//! `llama-server` on 127.0.0.1. Nothing is downloaded or started except by
//! an explicit call (the app's button, `kai models install|start`).
//!
//! Layout under [`LocalManager::default_root`]:
//! ```text
//! runtime/<build>/...          unpacked llama.cpp release
//! runtime/current.json         which build, and the server's path in it
//! models/<id>/<file>.gguf      model and vision projector
//! models/<id>/entry.json       the catalog entry it was installed from
//! models/<id>/.complete        written last; only then is it "installed"
//! downloads/                   runtime archives while downloading
//! server.json, logs/           the running server
//! ```

pub mod disk;
pub mod download;
pub mod hardware;
pub mod hub;
pub mod runtime;
pub mod server;

pub use download::{CancelToken, Cancelled};
pub use hardware::detect_hardware;
pub use runtime::{RuntimeSpec, Variant};
pub use server::{LocalServer, ServerInfo, ServerStatus};

use kai_core::models::ModelEntry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Install progress. `stage` is `"runtime"`, `"model"`, `"verify"` or `"done"`.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub stage: &'static str,
    pub bytes: u64,
    pub total: Option<u64>,
    pub message: String,
}

/// The progress callback: called from the install task, so `Send + Sync`.
pub type ProgressFn<'a> = &'a (dyn Fn(Progress) + Send + Sync);

#[derive(Debug, Serialize, Deserialize)]
struct CurrentRuntime {
    name: String,
    /// The server binary, relative to the build's folder.
    server: PathBuf,
}

/// Downloads, installs and runs local models. Cheap to clone; clones share
/// the cancel flag, so `cancel()` on any clone stops a running install.
#[derive(Clone)]
pub struct LocalManager {
    root: PathBuf,
    cancel: CancelToken,
    runtime: Option<RuntimeSpec>,
    health_timeout: Duration,
    http: reqwest::Client,
}

impl LocalManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            cancel: CancelToken::new(),
            runtime: None,
            health_timeout: Duration::from_secs(600),
            http: download::download_client(),
        }
    }

    /// At [`LocalManager::default_root`].
    pub fn open_default() -> Self {
        Self::new(Self::default_root())
    }

    /// Linux: `$XDG_DATA_HOME/kai/local` or `~/.local/share/kai/local`;
    /// Windows: `%LOCALAPPDATA%\Kai\local`. `KAI_LOCAL_DIR` overrides.
    pub fn default_root() -> PathBuf {
        if let Some(d) = std::env::var_os("KAI_LOCAL_DIR").filter(|d| !d.is_empty()) {
            return PathBuf::from(d);
        }
        if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join("Kai")
                .join("local")
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .filter(|d| !d.is_empty())
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
                .unwrap_or_default()
                .join("kai")
                .join("local")
        }
    }

    /// Use this runtime build instead of the pinned one for this hardware
    /// (tests, or a user who wants a different build).
    pub fn with_runtime(mut self, spec: RuntimeSpec) -> Self {
        self.runtime = Some(spec);
        self
    }

    /// How long `start()` waits for the model to load (default 10 minutes:
    /// a 19 GB model from a slow disk takes a while).
    pub fn with_health_timeout(mut self, t: Duration) -> Self {
        self.health_timeout = t;
        self
    }

    pub fn health_timeout(&self) -> Duration {
        self.health_timeout
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Stop a running `install()` at the next chunk; partial files are kept.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn cancel_token(&self) -> CancelToken {
        self.cancel.clone()
    }

    pub fn model_dir(&self, id: &str) -> PathBuf {
        self.root.join("models").join(id)
    }

    pub fn is_installed(&self, id: &str) -> bool {
        safe_id(id) && self.model_dir(id).join(".complete").is_file()
    }

    /// Installed model ids, sorted.
    pub fn installed(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.root.join("models"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|id| self.is_installed(id))
            .collect();
        v.sort();
        v
    }

    /// The catalog entry a model was installed from (survives catalog edits).
    pub fn installed_entry(&self, id: &str) -> Option<ModelEntry> {
        let text = std::fs::read_to_string(self.model_dir(id).join("entry.json")).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Delete a model's files (stopping its server first). Partial downloads
    /// of it go too.
    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        anyhow::ensure!(safe_id(id), "bad model id {id:?}");
        if self.running().is_some_and(|s| s.id == id) {
            self.stop()?;
        }
        let dir = self.model_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// The runtime build `install()` will use.
    pub fn runtime_spec(&self) -> anyhow::Result<RuntimeSpec> {
        match &self.runtime {
            Some(s) => Ok(s.clone()),
            None => runtime::for_hardware(&detect_hardware()),
        }
    }

    /// The installed `llama-server`, if any.
    pub fn server_path(&self) -> Option<PathBuf> {
        let cur = self.current_runtime()?;
        let p = self.root.join("runtime").join(&cur.name).join(cur.server);
        p.is_file().then_some(p)
    }

    fn current_runtime(&self) -> Option<CurrentRuntime> {
        let text = std::fs::read_to_string(self.root.join("runtime/current.json")).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn runtime_ready(&self, spec: &RuntimeSpec) -> bool {
        self.current_runtime().is_some_and(|c| c.name == spec.name) && self.server_path().is_some()
    }

    /// Bytes still to download for `entry` (runtime included).
    pub fn bytes_to_download(&self, entry: &ModelEntry) -> anyhow::Result<u64> {
        let spec = self.runtime_spec()?;
        let rt = if self.runtime_ready(&spec) {
            0
        } else {
            spec.total_bytes()
        };
        let dir = self.model_dir(&entry.id);
        let models: u64 = entry
            .files
            .iter()
            .map(|f| {
                let have = download::bytes_present(&dir.join(f.file_name()));
                f.size_bytes.unwrap_or(0).saturating_sub(have)
            })
            .sum();
        Ok(rt + models)
    }

    /// Download the runtime (when missing) and the model's files, verify
    /// them, and mark the model installed. Resumes partial downloads.
    /// Returns [`Cancelled`] (as the error) after [`LocalManager::cancel`].
    pub async fn install(
        &self,
        entry: &ModelEntry,
        progress: ProgressFn<'_>,
    ) -> anyhow::Result<()> {
        self.cancel.reset();
        let problems = entry.problems();
        anyhow::ensure!(problems.is_empty(), "{}: {}", entry.id, problems.join("; "));
        for file in &entry.files {
            if file.url.starts_with("https://") {
                anyhow::ensure!(
                    file.size_bytes.is_some() && file.sha256.is_some(),
                    "{} has no size or SHA-256; refusing an unverified remote model",
                    file.url
                );
            }
        }
        std::fs::create_dir_all(&self.root)?;
        let spec = self.runtime_spec()?;
        let need_runtime = !self.runtime_ready(&spec);
        // Archives and their unpacked copy for a moment: three times the size.
        let needed = self.bytes_to_download(entry)?
            + if need_runtime {
                spec.total_bytes() * 2
            } else {
                0
            };
        disk::check_space(needed, disk::free_bytes(&self.root)?, &self.root)?;
        if need_runtime {
            self.install_runtime(&spec, progress).await?;
        }
        let dir = self.model_dir(&entry.id);
        std::fs::create_dir_all(&dir)?;
        let total = entry.total_bytes();
        let mut done = 0u64;
        for f in &entry.files {
            let name = f.file_name();
            let dest = dir.join(&name);
            if download::part_path(&dest).exists() {
                progress(Progress {
                    stage: "verify",
                    bytes: done,
                    total,
                    message: format!("Checking the part of {name} already downloaded"),
                });
            }
            let message = format!("Downloading {name}");
            let mut cb = |b: u64, _: Option<u64>| {
                progress(Progress {
                    stage: "model",
                    bytes: done + b,
                    total,
                    message: message.clone(),
                })
            };
            download::fetch(
                &self.http,
                &f.url,
                &dest,
                f.sha256.as_deref(),
                f.size_bytes,
                &self.cancel,
                &mut cb,
            )
            .await?;
            done += std::fs::metadata(&dest)?.len();
            progress(Progress {
                stage: "verify",
                bytes: done,
                total,
                message: match &f.sha256 {
                    Some(_) => format!("{name}: checksum verified"),
                    None => format!("{name}: downloaded (no checksum published)"),
                },
            });
        }
        std::fs::write(dir.join("entry.json"), serde_json::to_string_pretty(entry)?)?;
        std::fs::write(dir.join(".complete"), b"")?;
        progress(Progress {
            stage: "done",
            bytes: done,
            total,
            message: format!("{} is ready", entry.label),
        });
        Ok(())
    }

    async fn install_runtime(
        &self,
        spec: &RuntimeSpec,
        progress: ProgressFn<'_>,
    ) -> anyhow::Result<()> {
        let dl = self.root.join("downloads");
        let total = Some(spec.total_bytes()).filter(|t| *t > 0);
        let mut done = 0u64;
        let mut archives = Vec::new();
        for a in &spec.assets {
            let name = a.file_name();
            let dest = dl.join(&name);
            let message = format!("Downloading llama.cpp ({name})");
            let mut cb = |b: u64, _: Option<u64>| {
                progress(Progress {
                    stage: "runtime",
                    bytes: done + b,
                    total,
                    message: message.clone(),
                })
            };
            download::fetch(
                &self.http,
                &a.url,
                &dest,
                a.sha256.as_deref(),
                a.size_bytes,
                &self.cancel,
                &mut cb,
            )
            .await?;
            done += std::fs::metadata(&dest)?.len();
            archives.push(dest);
        }
        progress(Progress {
            stage: "runtime",
            bytes: done,
            total,
            message: "Unpacking llama.cpp".into(),
        });
        let rt = self.root.join("runtime");
        let tmp = rt.join(format!(".unpack-{}", spec.name));
        let final_dir = rt.join(&spec.name);
        let exe = spec.server_exe.clone();
        let (tmp2, archives2) = (tmp.clone(), archives.clone());
        let server = tokio::task::spawn_blocking(move || -> anyhow::Result<PathBuf> {
            let _ = std::fs::remove_dir_all(&tmp2);
            for a in &archives2 {
                runtime::unpack(a, &tmp2)?;
            }
            let s = runtime::find_server(&tmp2, &exe)
                .ok_or_else(|| anyhow::anyhow!("{exe} is not in the downloaded archive"))?;
            Ok(s.strip_prefix(&tmp2)?.to_path_buf())
        })
        .await??;
        if final_dir.exists() {
            std::fs::remove_dir_all(&final_dir)?;
        }
        std::fs::rename(&tmp, &final_dir)?;
        let cur = CurrentRuntime {
            name: spec.name.clone(),
            server,
        };
        std::fs::write(rt.join("current.json"), serde_json::to_string_pretty(&cur)?)?;
        for a in archives {
            let _ = std::fs::remove_file(a);
        }
        Ok(())
    }
}

/// Ids become folder names: letters, digits, `-`, `_`, `.` only, no `..`.
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains("..")
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_cannot_escape_the_models_folder() {
        assert!(safe_id("gemma-4-e4b-it-q4"));
        for bad in ["", "..", "../x", "a/b", "a\\b", "c:x"] {
            assert!(!safe_id(bad), "{bad}");
        }
        let m = LocalManager::new("/nonexistent-kai-root");
        assert!(m.installed().is_empty());
        assert!(m.remove("../etc").is_err());
    }

    #[test]
    fn default_root_follows_the_platform() {
        let r = LocalManager::default_root();
        if std::env::var_os("KAI_LOCAL_DIR").is_none() {
            assert!(r.ends_with(if cfg!(windows) {
                "Kai/local"
            } else {
                "kai/local"
            }));
        }
    }
}
