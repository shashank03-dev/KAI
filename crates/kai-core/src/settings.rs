//! User settings: model endpoints and ids, safety, limits. Stored as JSON in
//! the user's config folder. API keys are never stored here; they live in the
//! OS keychain (see `kai_os::secrets`).

use crate::client::{
    FailoverClient, GeminiClient, MockClient, ModelClient, OpenAiClient, VllmClient,
    gateway_model_id,
};
use crate::risk::ApprovalLevel;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// The models Kai knows. Gemma 4 ids are the Gemini API names; the vLLM server
/// serves the same names (`--served-model-name`) and the AI Gateway maps them to
/// `google/<id>`. Comparison models are AI Gateway ids (any Gateway id works as
/// `fast_model`/`careful_model`; these are the ones the app offers).
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
    ModelInfo {
        id: "openai/gpt-5-mini",
        label: "GPT-5 mini · comparison (Gateway)",
        role: ModelRole::Comparison,
    },
    ModelInfo {
        id: "openai/gpt-5",
        label: "GPT-5 · comparison (Gateway)",
        role: ModelRole::Comparison,
    },
    ModelInfo {
        id: "openai/gpt-4.1",
        label: "GPT-4.1 · comparison (Gateway)",
        role: ModelRole::Comparison,
    },
    ModelInfo {
        id: "openai/gpt-4.1-mini",
        label: "GPT-4.1 mini · comparison (Gateway)",
        role: ModelRole::Comparison,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    Fast,
    Careful,
    /// Not Gemma: offered for side-by-side runs (Gateway or OpenAI backends).
    Comparison,
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
    /// Vercel AI Gateway (OpenAI-compatible): Gemma 4 and comparison models.
    Gateway,
    /// OpenAI directly (comparison runs).
    OpenAi,
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
    /// Record each run as training data under [`Settings::runs_dir`]
    /// (prompts, raw replies, the screenshots sent; `docs/DATASET.md`).
    /// Off by default; local only, never uploaded.
    pub record_traces: bool,
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
            record_traces: false,
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
    /// `AI_GATEWAY_API_KEY`.
    pub gateway: Option<String>,
    /// `OPENAI_API_KEY`.
    pub openai: Option<String>,
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

    /// Recorded runs (training data) when `record_traces` is on.
    pub fn runs_dir() -> PathBuf {
        Self::dir().join("runs")
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
            let gateway = || -> anyhow::Result<OpenAiClient> {
                let key = keys
                    .gateway
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("no AI Gateway key set (AI_GATEWAY_API_KEY)"))?;
                Ok(OpenAiClient::gateway(key).with_model(gateway_model_id(model)))
            };
            let openai = || -> anyhow::Result<OpenAiClient> {
                let key = keys
                    .openai
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("no OpenAI API key set (OPENAI_API_KEY)"))?;
                anyhow::ensure!(
                    !model.contains('/') || model.starts_with("openai/"),
                    "model {model} is not an OpenAI model"
                );
                Ok(OpenAiClient::openai(key).with_model(model.trim_start_matches("openai/")))
            };
            Ok(match self.backend {
                BackendChoice::Vllm => Box::new(vllm()?),
                BackendChoice::Gemini => Box::new(gemini()?),
                BackendChoice::Gateway => Box::new(gateway()?),
                BackendChoice::OpenAi => Box::new(openai()?),
                BackendChoice::Mock => Box::new(MockClient::always(
                    r#"{"thought":"mock backend","action":"done","risk":"none"}"#,
                )),
                BackendChoice::Both => {
                    // Droplet first, then Gemini, then the AI Gateway: each
                    // one that is configured backs up the one before it.
                    let mut chain: Vec<Box<dyn ModelClient>> = Vec::new();
                    let first_err = match vllm() {
                        Ok(v) => {
                            chain.push(Box::new(v));
                            None
                        }
                        Err(e) => Some(e),
                    };
                    if let Ok(g) = gemini() {
                        chain.push(Box::new(g));
                    }
                    if let Ok(g) = gateway() {
                        chain.push(Box::new(g));
                    }
                    let mut it = chain.into_iter().rev();
                    let Some(mut client) = it.next() else {
                        let e = first_err.unwrap_or_else(|| anyhow::anyhow!("no backend"));
                        return Err(e.context(
                            "set up the DigitalOcean server or a Gemini key (or an AI Gateway key)",
                        ));
                    };
                    for primary in it {
                        client = Box::new(FailoverClient::new(primary, Some(client)));
                    }
                    client
                }
            })
        };
        let fast = make(&self.fast_model)?;
        let careful = if self.careful_model != self.fast_model {
            make(&self.careful_model).ok().map(|c| {
                // On the Gateway/OpenAI the careful model may be off-limits for
                // this account (free tier: 403 for Gemma 4 31B). Back it with
                // the fast model so a careful step never fails the run.
                if matches!(self.backend, BackendChoice::Gateway | BackendChoice::OpenAi)
                    && let Ok(f) = make(&self.fast_model)
                {
                    Box::new(FailoverClient::new(c, Some(f))) as Box<dyn ModelClient>
                } else {
                    c
                }
            })
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
                ..Default::default()
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
                ..Default::default()
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
    fn gateway_backend_maps_ids_and_backs_careful_with_fast() {
        let s = Settings {
            backend: BackendChoice::Gateway,
            ..Default::default()
        };
        let keys = Keys {
            gateway: Some("g".into()),
            ..Default::default()
        };
        let (fast, careful) = s.clients(&keys).unwrap();
        assert_eq!(fast.backend(), Backend::OpenAi);
        assert_eq!(careful.unwrap().backend(), Backend::OpenAi);
        let err = s.clients(&Keys::default()).err().unwrap();
        assert!(err.to_string().contains("AI_GATEWAY_API_KEY"));
        assert_eq!(gateway_model_id(MODELS[0].id), "google/gemma-4-26b-a4b-it");
        assert_eq!(gateway_model_id("openai/gpt-5-mini"), "openai/gpt-5-mini");
    }

    #[test]
    fn openai_backend_takes_openai_ids_only() {
        let keys = Keys {
            openai: Some("o".into()),
            ..Default::default()
        };
        let s = Settings {
            backend: BackendChoice::OpenAi,
            fast_model: "openai/gpt-5-mini".into(),
            careful_model: "gpt-5".into(),
            ..Default::default()
        };
        let (fast, careful) = s.clients(&keys).unwrap();
        assert_eq!(fast.backend(), Backend::OpenAi);
        assert!(careful.is_some());
        let bad = Settings {
            backend: BackendChoice::OpenAi,
            ..Default::default()
        };
        assert!(bad.clients(&keys).is_ok()); // plain gemma id: OpenAI will 404 at call time
        let bad = Settings {
            backend: BackendChoice::OpenAi,
            fast_model: "google/gemma-4-26b-a4b-it".into(),
            ..Default::default()
        };
        assert!(bad.clients(&keys).is_err());
    }

    #[test]
    fn both_falls_back_to_gateway_when_it_is_the_only_key() {
        let (fast, _) = Settings::default()
            .clients(&Keys {
                gateway: Some("g".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(fast.backend(), Backend::OpenAi);
    }

    #[test]
    fn comparison_models_are_gateway_ids() {
        let cmp: Vec<_> = MODELS
            .iter()
            .filter(|m| m.role == ModelRole::Comparison)
            .collect();
        assert!(cmp.len() >= 3);
        assert!(cmp.iter().all(|m| m.id.starts_with("openai/")));
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
