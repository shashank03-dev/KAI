//! Local open models: the catalog (data, not code) and the recommendation
//! for this computer. Adding a model is one JSON entry in `catalog.json`, or
//! in the user's `<config>/models.json`, which is merged on top (same `id`
//! replaces, a new `id` is added). See `docs/LOCAL_MODELS.md`.
//!
//! OS-free: the hardware numbers come from `kai_os::local::detect_hardware`,
//! downloads and the server from `kai_os::local::LocalManager`.

pub mod hub;
#[cfg(test)]
mod hub_tests;
mod rank;
#[cfg(test)]
mod rank_tests;

pub use rank::{Fit, Ranked, rank};

use crate::client::OpenAiClient;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The built-in catalog, embedded at build time.
pub const BUILTIN_CATALOG: &str = include_str!("catalog.json");

/// Default port for the local `llama-server` (127.0.0.1 only).
pub const DEFAULT_PORT: u16 = 8098;

/// One downloadable file of a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelFile {
    pub url: String,
    /// Lower-case hex; checked after download when present.
    #[serde(default)]
    pub sha256: Option<String>,
    /// `"model"` (the weights) or `"mmproj"` (the vision projector).
    pub role: String,
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

impl ModelFile {
    /// The file name on disk: the last path segment of the URL.
    pub fn file_name(&self) -> String {
        let path = self.url.split(['?', '#']).next().unwrap_or(&self.url);
        path.rsplit('/').next().unwrap_or(path).to_string()
    }
}

/// One model in the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEntry {
    /// Stable id, e.g. `gemma-4-e4b-it-q4`.
    pub id: String,
    /// e.g. `gemma-4`.
    pub family: String,
    pub label: String,
    pub maker: String,
    /// SPDX id, e.g. `Apache-2.0`.
    pub license: String,
    /// Total parameters, billions.
    pub params_b: f32,
    /// Parameters that work on each token (mixture of experts, or Gemma's
    /// "effective" size), billions. `None` = dense, all of `params_b`.
    #[serde(default)]
    pub active_params_b: Option<f32>,
    /// e.g. `Q4_0`, `Q4_K_M`.
    pub quant: String,
    pub vision: bool,
    /// The context Kai asks the server for (tokens).
    pub context: u32,
    /// All files together, GB (10^9 bytes).
    pub download_gb: f32,
    /// Memory the model needs when it runs on the CPU: files + KV cache at
    /// `context` + runtime overhead, GB. The OS reserve is added by [`rank`].
    pub min_ram_gb: f32,
    /// Graphics memory for full GPU offload, GB.
    #[serde(default)]
    pub min_vram_gb: Option<f32>,
    pub files: Vec<ModelFile>,
    /// `llama.cpp`.
    pub runtime: String,
    /// The name the server answers to (`--alias`) and Kai sends as `model`.
    pub served_name: String,
    #[serde(default)]
    pub notes: String,
    /// `recommended`, `experimental` or `untested`.
    pub computer_use: String,
    /// Its maker trained it to operate computers (Holo, Fara). Shown in the
    /// store and breaks ties between untested models; it is not a Kai
    /// measurement (that is `computer_use`).
    #[serde(default)]
    pub agent_trained: bool,
}

impl ModelEntry {
    /// Parameters that work per token.
    pub fn active_b(&self) -> f32 {
        self.active_params_b.unwrap_or(self.params_b)
    }

    pub fn model_file(&self) -> Option<&ModelFile> {
        self.files.iter().find(|f| f.role == "model")
    }

    pub fn mmproj_file(&self) -> Option<&ModelFile> {
        self.files.iter().find(|f| f.role == "mmproj")
    }

    /// Bytes to download, when every file's size is known.
    pub fn total_bytes(&self) -> Option<u64> {
        self.files.iter().map(|f| f.size_bytes).sum()
    }

    /// What the status bar shows for a run on this model.
    pub fn backend_label(&self) -> String {
        format!("This computer ({})", self.label)
    }

    /// Problems that make the entry unusable (empty = fine).
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        let ok_id = !self.id.is_empty()
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !ok_id {
            p.push(format!(
                "id {:?} must be letters, digits, '-', '_' or '.'",
                self.id
            ));
        }
        if self.files.iter().filter(|f| f.role == "model").count() != 1 {
            p.push("needs exactly one file with role \"model\"".into());
        }
        for f in &self.files {
            // https only; plain http just for a mirror on this machine.
            if !(f.url.starts_with("https://") || f.url.starts_with("http://127.0.0.1:")) {
                p.push(format!("{} is not an https URL", f.url));
            }
            if !matches!(f.role.as_str(), "model" | "mmproj") {
                p.push(format!("role {:?} must be \"model\" or \"mmproj\"", f.role));
            }
            if let Some(h) = &f.sha256
                && (h.len() != 64 || !h.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')))
            {
                p.push(format!(
                    "sha256 of {} must be 64 lower-case hex digits",
                    f.url
                ));
            }
            let name = f.file_name();
            if name.is_empty() || name.contains("..") || name.contains('\\') {
                p.push(format!("{} has no usable file name", f.url));
            }
        }
        if self.runtime != "llama.cpp" {
            p.push(format!(
                "runtime {:?} is not supported (llama.cpp)",
                self.runtime
            ));
        }
        if self.served_name.is_empty() {
            p.push("served_name is empty".into());
        }
        if !matches!(
            self.computer_use.as_str(),
            "recommended" | "experimental" | "untested"
        ) {
            p.push(format!("computer_use {:?} is unknown", self.computer_use));
        }
        if self.context < 2048 || self.min_ram_gb <= 0.0 {
            p.push("context or min_ram_gb is too small".into());
        }
        p
    }
}

/// What this computer has, as far as the recommendation cares.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HardwareSummary {
    pub ram_gb: f32,
    pub cpu_threads: u32,
    /// Dedicated graphics memory of the biggest GPU, GB.
    pub gpu_vram_gb: Option<f32>,
    pub gpu_name: Option<String>,
    /// `linux`, `windows`.
    pub os: String,
    /// `x86_64`, `aarch64`.
    pub arch: String,
}

impl HardwareSummary {
    /// One line for `kai doctor` and the app's hardware card.
    pub fn describe(&self) -> String {
        let gpu = match (&self.gpu_name, self.gpu_vram_gb) {
            (Some(n), Some(v)) => format!("{n} ({v:.0} GB)"),
            (Some(n), None) => format!("{n} (graphics memory unknown)"),
            (None, Some(v)) => format!("GPU ({v:.0} GB)"),
            (None, None) => "no dedicated GPU found".into(),
        };
        format!(
            "{:.0} GB RAM · {} CPU threads · {gpu} · {} {}",
            self.ram_gb, self.cpu_threads, self.os, self.arch
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub version: u32,
    pub models: Vec<ModelEntry>,
}

impl Catalog {
    /// The built-in catalog only.
    pub fn builtin() -> Self {
        Self::from_json(BUILTIN_CATALOG).expect("built-in catalog.json is valid")
    }

    /// The built-in catalog with `<config>/models.json` merged on top.
    pub fn load() -> Self {
        let mut c = Self::builtin();
        let path = Self::user_file();
        if let Err(e) = c.merge_file(&path) {
            log::warn!("ignoring {}: {e:#}", path.display());
        }
        c
    }

    /// Parse and validate: entries with problems are dropped (logged).
    pub fn from_json(text: &str) -> anyhow::Result<Self> {
        let mut c: Catalog = serde_json::from_str(text)?;
        c.models.retain(|m| {
            let p = m.problems();
            if !p.is_empty() {
                log::warn!("catalog entry {} skipped: {}", m.id, p.join("; "));
            }
            p.is_empty()
        });
        let mut seen = std::collections::HashSet::new();
        c.models.retain(|m| seen.insert(m.id.clone()));
        Ok(c)
    }

    /// Merge a user file if it exists: same id replaces, new ids are added.
    pub fn merge_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Ok(());
        };
        self.merge(Self::from_json(&text)?);
        Ok(())
    }

    pub fn merge(&mut self, other: Catalog) {
        for m in other.models {
            match self.models.iter_mut().find(|x| x.id == m.id) {
                Some(slot) => *slot = m,
                None => self.models.push(m),
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }

    /// An entry with the same model weights as `e` (by SHA-256), if any.
    pub fn same_weights(&self, e: &ModelEntry) -> Option<&ModelEntry> {
        let sha = e.model_file()?.sha256.as_deref()?;
        self.models
            .iter()
            .find(|m| m.model_file().and_then(|f| f.sha256.as_deref()) == Some(sha))
    }

    /// Add a pulled entry to the user catalog at `path`, unless these
    /// weights are already in this catalog under another id.
    pub fn add_pulled(&self, path: &Path, e: &ModelEntry) -> anyhow::Result<Pulled> {
        if let Some(same) = self.same_weights(e).filter(|s| s.id != e.id) {
            return Ok(Pulled::AlreadyHave(same.id.clone()));
        }
        Self::save_user_entry(path, e)?;
        Ok(Pulled::Added)
    }

    /// The user's own catalog file, merged on top of the built-in one.
    pub fn user_file() -> std::path::PathBuf {
        crate::settings::Settings::dir().join("models.json")
    }

    /// Add `entry` to the user catalog at `path` (same id replaces), keeping
    /// every other entry there. Written to a temporary file first, so a
    /// crash never leaves half a catalog.
    pub fn save_user_entry(path: &Path, entry: &ModelEntry) -> anyhow::Result<()> {
        let mut user = match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };
        user.version = 1;
        user.merge(Catalog {
            version: 1,
            models: vec![entry.clone()],
        });
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&user)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// What [`Catalog::add_pulled`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pulled {
    Added,
    /// The same weights are already in the catalog under this id.
    AlreadyHave(String),
}

/// The OpenAI-compatible base URL of the local server.
pub fn local_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

/// The client for `BackendChoice::Local`: the entry's `served_name` at
/// `http://127.0.0.1:<local_port>/v1`, no key.
pub fn local_client(s: &crate::settings::Settings) -> anyhow::Result<OpenAiClient> {
    local_client_from(&Catalog::load(), s)
}

pub fn local_client_from(
    catalog: &Catalog,
    s: &crate::settings::Settings,
) -> anyhow::Result<OpenAiClient> {
    anyhow::ensure!(
        !s.local_model.is_empty(),
        "no local model chosen: see `kai models list`, then `kai models install <id>`"
    );
    let e = catalog
        .get(&s.local_model)
        .ok_or_else(|| anyhow::anyhow!("{} is not in the model catalog", s.local_model))?;
    Ok(OpenAiClient::local(
        local_base_url(s.local_port),
        &e.served_name,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{Backend, ModelClient};
    use crate::settings::{BackendChoice, Keys, Settings};

    #[test]
    fn local_backend_needs_no_key_and_has_no_careful_slot() {
        let s = Settings {
            backend: BackendChoice::Local,
            local_model: "gemma-4-e4b-it-q4".into(),
            ..Default::default()
        };
        assert_eq!(s.local_port, DEFAULT_PORT);
        let (fast, careful) = s.clients(&Keys::default()).unwrap();
        assert_eq!(fast.backend(), Backend::Local);
        assert!(careful.is_none());
        let c = local_client_from(&Catalog::builtin(), &s).unwrap();
        assert_eq!(c.model(), "gemma-4-e4b-it");
        assert_eq!(c.backend(), Backend::Local);
        let none = Settings {
            backend: BackendChoice::Local,
            ..Default::default()
        };
        let e = none.clients(&Keys::default()).err().unwrap().to_string();
        assert!(e.contains("kai models"), "{e}");
        assert_eq!(local_base_url(8098), "http://127.0.0.1:8098/v1");
    }

    #[test]
    fn builtin_catalog_is_valid_and_complete() {
        let c = Catalog::builtin();
        let raw: serde_json::Value = serde_json::from_str(BUILTIN_CATALOG).unwrap();
        assert_eq!(c.models.len(), raw["models"].as_array().unwrap().len());
        for id in [
            "gemma-4-e2b-it-q4",
            "gemma-4-e4b-it-q4",
            "gemma-4-26b-a4b-it-q4",
            "gemma-4-31b-it-q4",
        ] {
            let m = c.get(id).unwrap();
            assert!(m.vision && m.mmproj_file().is_some(), "{id}");
        }
        for m in &c.models {
            assert!(m.problems().is_empty(), "{}: {:?}", m.id, m.problems());
            assert!(
                m.files
                    .iter()
                    .all(|f| f.sha256.is_some() && f.size_bytes.is_some())
            );
            // download_gb matches the files, and memory covers files + overhead.
            let gb = m.total_bytes().unwrap() as f32 / 1e9;
            assert!((gb - m.download_gb).abs() < 0.02, "{}: {gb}", m.id);
            assert!(m.min_ram_gb >= m.download_gb + 1.0, "{}", m.id);
            assert!(m.min_vram_gb.unwrap() >= m.download_gb + 1.0, "{}", m.id);
        }
    }

    #[test]
    fn user_file_overrides_and_adds() {
        let mut c = Catalog::builtin();
        let mut e = c.get("gemma-4-e2b-it-q4").unwrap().clone();
        e.label = "Mine".into();
        let mut extra = e.clone();
        extra.id = "my-model".into();
        let mut bad = e.clone();
        bad.id = "bad".into();
        bad.files[0].url = "http://insecure/x.gguf".into();
        let text = serde_json::json!({ "models": [e, extra, bad] }).to_string();
        let n = c.models.len();
        c.merge(Catalog::from_json(&text).unwrap());
        assert_eq!(c.models.len(), n + 1);
        assert_eq!(c.get("gemma-4-e2b-it-q4").unwrap().label, "Mine");
        assert!(c.get("bad").is_none());
        assert_eq!(
            c.get("my-model").unwrap().backend_label(),
            "This computer (Mine)"
        );
    }

    #[test]
    fn file_names_come_from_urls() {
        let f = ModelFile {
            url: "https://h/a/b/model-Q4_0.gguf?download=true".into(),
            sha256: None,
            role: "model".into(),
            size_bytes: None,
        };
        assert_eq!(f.file_name(), "model-Q4_0.gguf");
    }
}
