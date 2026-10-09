//! User settings: model endpoints and ids, safety, limits. Stored as JSON in
//! the user's config folder. API keys are never stored here; they live in the
//! OS keychain (see `kai_os::secrets`).

use crate::client::{FailoverClient, GeminiClient, MockClient, ModelClient, VllmClient};
use crate::risk::ApprovalLevel;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// The Gemma 4 family Kai knows. Ids are the Gemini API names; the vLLM server
/// serves the same names (`--served-model-name`).
pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "gemma-4-26b-a4b-it",
        label: "Gemma 4 26B A4B · fast",
        role: ModelRole::Fast,
    },
    ModelInfo {
        id: "gemma-4-31b-it",
        label: "Gemma 4 31B · careful",
        role: ModelRole::Careful,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    Fast,
    Careful,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub role: ModelRole,
}

/// Which backend(s) to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackendChoice {
    /// The Droplet first, Gemini as automatic fallback. Recommended.
    #[default]
    Both,
    Vllm,
    Gemini,
    Mock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub backend: BackendChoice,
    /// e.g. `https://1-2-3-4.sslip.io/v1`
    pub vllm_endpoint: String,
    pub fast_model: String,
    pub careful_model: String,
    pub approval: ApprovalLevel,
    pub kill_hotkey: String,
    pub launcher_hotkey: String,
    pub max_steps: u32,
    pub max_minutes: u32,
    /// Parallel terminal sub-agents.
    pub parallelism: usize,
    /// Plan tasks into sub-agents when they look like several jobs.
    pub team_mode: bool,
    /// Droplet price for the cost estimator (US$ per hour).
    pub droplet_hourly_usd: f32,
    pub theme: String,
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backend: BackendChoice::Both,
            vllm_endpoint: String::new(),
            fast_model: MODELS[0].id.to_string(),
            careful_model: MODELS[1].id.to_string(),
            approval: ApprovalLevel::HighRiskOnly,
            kill_hotkey: "ctrl+alt+Escape".to_string(),
            launcher_hotkey: "ctrl+shift+Space".to_string(),
            max_steps: 40,
            max_minutes: 10,
            parallelism: 4,
            team_mode: true,
            droplet_hourly_usd: 3.39,
            theme: "system".to_string(),
            onboarded: false,
        }
    }
}

/// The fast client, and the careful one used after a failed step.
pub type Clients = (Box<dyn ModelClient>, Option<Box<dyn ModelClient>>);

/// Keys come from the keychain or the environment, never from the settings file.
#[derive(Debug, Clone, Default)]
pub struct Keys {
    pub vllm: Option<String>,
    pub gemini: Option<String>,
}

impl Settings {
    /// `~/.config/kai` on Linux, `%APPDATA%\Kai` on Windows.
    pub fn dir() -> PathBuf {
        if cfg!(windows) {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join("Kai")
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .unwrap_or_default()
                .join("kai")
        }
    }

    /// Where traces live (local only).
    pub fn traces_dir() -> PathBuf {
        Self::dir().join("traces")
    }

    /// The user's own skills.
    pub fn skills_dir() -> PathBuf {
        Self::dir().join("skills")
    }

    pub fn load() -> Self {
        let path = Self::dir().join("settings.json");
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::error!(
                    "settings at {} are unreadable ({e}); using defaults",
                    path.display()
                );
                Self::default()
            }),
            Err(_) => Self::default(),
        }
        .with_env()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(Self::dir())?;
        std::fs::write(
            Self::dir().join("settings.json"),
            serde_json::to_string_pretty(self)?,
        )?;
        Ok(())
    }

    /// Environment overrides, for CI and the deploy scripts' `endpoint.env`.
    fn with_env(mut self) -> Self {
        if let Ok(e) = std::env::var("KAI_ENDPOINT")
            && !e.is_empty()
        {
            self.vllm_endpoint = e;
        }
        self
    }

    pub fn limits(&self) -> (u32, Duration) {
        (
            self.max_steps.clamp(1, 500),
            Duration::from_secs(self.max_minutes.clamp(1, 240) as u64 * 60),
        )
    }

    /// Build the fast and careful clients for the chosen backend.
    pub fn clients(&self, keys: &Keys) -> anyhow::Result<Clients> {
        let make = |model: &str| -> anyhow::Result<Box<dyn ModelClient>> {
            let vllm = || -> anyhow::Result<VllmClient> {
                anyhow::ensure!(
                    !self.vllm_endpoint.is_empty(),
                    "no DigitalOcean endpoint set"
                );
                let key = keys
                    .vllm
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("no DigitalOcean API key set"))?;
                Ok(VllmClient::new(&self.vllm_endpoint, key).with_model(model))
            };
            let gemini = || -> anyhow::Result<GeminiClient> {
                let key = keys
                    .gemini
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("no Gemini API key set"))?;
                Ok(GeminiClient::new(key).with_model(model))
            };
            Ok(match self.backend {
                BackendChoice::Vllm => Box::new(vllm()?),
                BackendChoice::Gemini => Box::new(gemini()?),
                BackendChoice::Mock => Box::new(MockClient::always(
                    r#"{"thought":"mock backend","action":"done","risk":"none"}"#,
                )),
                BackendChoice::Both => match (vllm(), gemini()) {
                    (Ok(v), Ok(g)) => Box::new(FailoverClient::new(Box::new(v), Some(Box::new(g)))),
                    (Ok(v), Err(_)) => Box::new(v),
                    (Err(_), Ok(g)) => Box::new(g),
                    (Err(e), Err(_)) => {
                        return Err(e.context("set up the DigitalOcean server or a Gemini key"));
                    }
                },
            })
        };
        let fast = make(&self.fast_model)?;
        let careful = if self.careful_model != self.fast_model {
            make(&self.careful_model).ok()
        } else {
            None
        };
        Ok((fast, careful))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Backend;

    #[test]
    fn defaults_are_safe() {
        let s = Settings::default();
        assert_eq!(s.approval, ApprovalLevel::HighRiskOnly);
        assert_eq!(s.max_steps, 40);
        assert_eq!(s.limits().1, Duration::from_secs(600));
        assert_eq!(s.fast_model, "gemma-4-26b-a4b-it");
    }

    #[test]
    fn unknown_fields_and_missing_fields_load() {
        let s: Settings =
            serde_json::from_str(r#"{"max_steps": 7, "future_field": true}"#).unwrap();
        assert_eq!(s.max_steps, 7);
        assert_eq!(s.parallelism, 4);
    }

    #[test]
    fn both_with_only_gemini_uses_gemini() {
        let s = Settings::default();
        let (fast, careful) = s
            .clients(&Keys {
                vllm: None,
                gemini: Some("k".into()),
            })
            .unwrap();
        assert_eq!(fast.backend(), Backend::Gemini);
        assert!(careful.is_some());
    }

    #[test]
    fn both_with_both_fails_over() {
        let s = Settings {
            vllm_endpoint: "https://x/v1".into(),
            ..Default::default()
        };
        let (fast, _) = s
            .clients(&Keys {
                vllm: Some("a".into()),
                gemini: Some("b".into()),
            })
            .unwrap();
        assert_eq!(fast.backend(), Backend::Vllm);
    }

    #[test]
    fn nothing_configured_is_a_clear_error() {
        let err = Settings::default().clients(&Keys::default()).err().unwrap();
        assert!(
            err.to_string()
                .contains("set up the DigitalOcean server or a Gemini key")
        );
    }

    #[test]
    fn limits_are_clamped() {
        let s = Settings {
            max_steps: 0,
            max_minutes: 100_000,
            ..Default::default()
        };
        assert_eq!(s.limits(), (1, Duration::from_secs(240 * 60)));
    }
}
