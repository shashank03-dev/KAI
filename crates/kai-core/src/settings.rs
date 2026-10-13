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
    /// A model on this computer (`llama-server`, see `crate::models`).
    Local,
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
    /// Where the executor's local decision model (jevos-v4, `./jev serve`)
    /// listens. Env `KAI_EXECUTOR_URL` overrides; `JEV_API_KEY`, if set, is
    /// sent as a bearer key. Used only when a run turns the executor on.
    pub executor_endpoint: String,
    /// Let a fast local decision model (jevos) carry out Gemma's plan
    /// (docs/EXECUTOR.md). Off until measured.
    pub use_executor: bool,
    /// Check the result before accepting "done": the outcome ledger and
    /// DONE gate (docs/VERIFICATION.md). Off until measured.
    pub verify_outcomes: bool,
    /// Summarise older steps for the model instead of dropping them
    /// (macro-action history). Off until measured.
    pub macro_history: bool,
    /// Replay remembered procedures for repeated tasks and record verified
    /// runs (docs/PROCEDURES.md). Off until measured.
    pub use_procedures: bool,
    /// Plan long tasks three ways first and follow the plan the others
    /// agree with (3 extra model calls, in parallel). Off until measured.
    pub best_of_plans: bool,
    /// When two steps change nothing, diagnose why and change course once
    /// before asking. Off until measured.
    pub recover: bool,
    /// Give the model a run memo, a short scratchpad shown back every step,
    /// so long runs keep what they read (docs/LONG_RUNS.md). Off until
    /// measured.
    pub memo: bool,
    /// Check the task's named output (a file, a folder in the file manager)
    /// before accepting "done" (docs/VERIFICATION.md). Off until measured.
    pub task_checks: bool,
    /// Do tasks in Kai's own browser over the DevTools Protocol instead of
    /// on the desktop (docs/BROWSER.md): background input, the page's own
    /// tree. Off by default; Linux for now.
    pub use_browser: bool,
    /// Catalog id of the model on this computer (`BackendChoice::Local`).
    pub local_model: String,
    /// Port of the local `llama-server` (127.0.0.1 only).
    pub local_port: u16,
    /// Experience memory: learn from mistakes on this computer (lessons in
    /// [`Settings::memory_dir`], shown to the model as hints; docs/MEMORY.md).
    pub learn: bool,
    /// After a failed run, ask the model once for up to two lessons (one
    /// extra model call per such run).
    pub reflect: bool,
    /// Skills the user switched off on the Skills page, by name.
    pub disabled_skills: Vec<String>,
    /// See-through window material (Mica on Windows 11, blur where the Linux
    /// compositor offers it). On by default on Windows only: without a
    /// compositor blur, Linux would show the raw desktop behind the text.
    pub glass: bool,
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
            executor_endpoint: crate::client::jev::LOCAL_BASE.to_string(),
            use_executor: false,
            verify_outcomes: false,
            macro_history: false,
            use_procedures: false,
            best_of_plans: false,
            recover: false,
            memo: false,
            task_checks: false,
            use_browser: false,
            local_model: String::new(),
            local_port: crate::models::DEFAULT_PORT,
            learn: true,
            reflect: false,
            disabled_skills: Vec::new(),
            glass: cfg!(windows),
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

    /// Experience memory (`lessons.jsonl`), local only.
    pub fn memory_dir() -> PathBuf {
        Self::dir().join("memory")
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
        if let Ok(e) = std::env::var("KAI_EXECUTOR_URL")
            && !e.trim().is_empty()
        {
            self.executor_endpoint = e.trim().to_string();
        }
        self
    }

    /// The executor's decision client: jevos at `executor_endpoint`, with
    /// `JEV_API_KEY` as bearer key only when that variable is set.
    pub fn executor_client(&self) -> crate::client::DecisionClient {
        let base = if self.executor_endpoint.trim().is_empty() {
            crate::client::jev::LOCAL_BASE
        } else {
            self.executor_endpoint.trim()
        };
        let c = crate::client::DecisionClient::local(base);
        match std::env::var("JEV_API_KEY") {
            Ok(k) if !k.trim().is_empty() => c.with_key(k.trim()),
            _ => c,
        }
    }

    pub fn limits(&self) -> (u32, Duration) {
        (
            self.max_steps.clamp(1, 500),
            Duration::from_secs(self.max_minutes.clamp(1, 240) as u64 * 60),
        )
    }

    /// The engine config these settings ask for: limits, approvals, memory
    /// and the experimental switches (each off unless turned on).
    pub fn engine_config(&self) -> crate::Config {
        let (max_steps, max_time) = self.limits();
        crate::Config {
            max_steps,
            max_time,
            approval: self.approval,
            executor: self.use_executor,
            outcomes: self.verify_outcomes,
            macro_history: self.macro_history,
            procedures: self.use_procedures,
            best_of: if self.best_of_plans { 3 } else { 0 },
            recover: self.recover,
            memo: self.memo,
            task_checks: self.task_checks,
            learn: self.learn,
            reflect: self.reflect,
            ..crate::Config::default()
        }
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
                BackendChoice::Local => Box::new(crate::models::local_client(self)?),
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
        let careful =
            if self.careful_model != self.fast_model && self.backend != BackendChoice::Local {
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
#[path = "settings_tests.rs"]
mod tests;
