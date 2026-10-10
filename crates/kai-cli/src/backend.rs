//! `--backend` / `--model` for `kai run`, `kai bench` and `kai smoke`, and the
//! key report for `kai doctor`. Everything goes through `Settings::clients()`
//! so a backend wired in kai-core works here without changes.

use crate::ab::{Ab, apply_ab};
use clap::{Args, ValueEnum};
use kai_core::Config;
use kai_core::settings::{BackendChoice, Keys, Settings};

/// Default model on the Vercel AI Gateway (its ids carry the vendor prefix).
pub const GATEWAY_DEFAULT_MODEL: &str = "google/gemma-4-26b-a4b-it";

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BackendArg {
    /// Whatever the settings file says (the app's choice).
    Auto,
    /// Your DigitalOcean vLLM server.
    Vllm,
    /// Google's Gemini API (Gemma 4).
    Gemini,
    /// Vercel AI Gateway, OpenAI-compatible: Gemma 4 and comparison models.
    Gateway,
    /// OpenAI directly (comparison runs).
    #[value(name = "openai")]
    OpenAi,
    /// OpenAI's own computer-use loop instead of Kai's harness.
    OpenaiNative,
    /// An open model on this computer (`kai models`); `--model <catalog id>`.
    Local,
    /// Recorded replies on a scripted screen; touches nothing (bench only).
    Mock,
    /// Recorded replies, executed for real on this desktop (bench only).
    Replay,
}

impl BackendArg {
    pub fn name(self) -> &'static str {
        match self {
            BackendArg::Auto => "auto",
            BackendArg::Vllm => "vllm",
            BackendArg::Gemini => "gemini",
            BackendArg::Gateway => "gateway",
            BackendArg::OpenAi => "openai",
            BackendArg::OpenaiNative => "openai-native",
            BackendArg::Local => "local",
            BackendArg::Mock => "mock",
            BackendArg::Replay => "replay",
        }
    }

    pub fn recorded(self) -> bool {
        matches!(self, BackendArg::Mock | BackendArg::Replay)
    }
}

/// Backend flags shared by `run`, `bench` and `smoke`.
#[derive(Args, Clone, Debug, Default)]
pub struct BackendOpts {
    /// Model backend (default: auto for run/smoke, mock for bench).
    #[arg(long, value_enum)]
    pub backend: Option<BackendArg>,
    /// Model id for the backend, e.g. google/gemma-4-26b-a4b-it or gpt-5-mini.
    /// Used for both the fast and the careful slot.
    #[arg(long)]
    pub model: Option<String>,
    #[command(flatten)]
    pub ab: Ab,
}

/// Settings with the chosen backend and model applied.
pub fn settings_for(b: BackendArg, model: Option<&str>) -> anyhow::Result<Settings> {
    let mut s = Settings::load();
    s.backend = match b {
        BackendArg::Auto => s.backend,
        BackendArg::Vllm => BackendChoice::Vllm,
        BackendArg::Gemini => BackendChoice::Gemini,
        BackendArg::Gateway => BackendChoice::Gateway,
        BackendArg::OpenAi | BackendArg::OpenaiNative => BackendChoice::OpenAi,
        BackendArg::Local => BackendChoice::Local,
        BackendArg::Mock | BackendArg::Replay => BackendChoice::Mock,
    };
    if b == BackendArg::Local || (b == BackendArg::Auto && s.backend == BackendChoice::Local) {
        // A catalog id, not a model name: the server answers to its alias.
        if let Some(m) = model {
            s.local_model = m.trim().to_string();
        }
        return Ok(s);
    }
    let model = match (b, model) {
        (_, Some(m)) => Some(m.trim().to_string()),
        (BackendArg::Gateway, None) => Some(GATEWAY_DEFAULT_MODEL.to_string()),
        (BackendArg::OpenAi, None) => {
            anyhow::bail!("--backend openai needs --model (for example --model gpt-5-mini)")
        }
        _ => None,
    };
    if let Some(m) = model {
        anyhow::ensure!(!m.is_empty(), "--model is empty");
        s.fast_model = m.clone();
        s.careful_model = m;
    }
    Ok(s)
}

/// The model id a run uses, for results files.
pub fn model_label(b: BackendArg, s: &Settings) -> String {
    if b.recorded() {
        "recorded".into()
    } else if s.backend == BackendChoice::Local {
        s.local_model.clone()
    } else {
        s.fast_model.clone()
    }
}

/// A clear error before any network call when the backend's key is missing.
pub fn require_key(b: BackendArg, keys: &Keys) -> anyhow::Result<()> {
    let (missing, which, env) = match b {
        BackendArg::Gateway => (keys.gateway.is_none(), "gateway", "AI_GATEWAY_API_KEY"),
        BackendArg::OpenAi | BackendArg::OpenaiNative => {
            (keys.openai.is_none(), "openai", "OPENAI_API_KEY")
        }
        BackendArg::Gemini => (keys.gemini.is_none(), "gemini", "GEMINI_API_KEY"),
        BackendArg::Vllm => (keys.vllm.is_none(), "vllm", "VLLM_API_KEY"),
        _ => (false, "", ""),
    };
    anyhow::ensure!(
        !missing,
        "no {which} API key: run `kai key {which}` or set {env}"
    );
    Ok(())
}

/// Engine config from settings plus the A/B switches. The bool says whether
/// the switches reached the engine (see [`apply_ab`]).
pub fn config(s: &Settings, ab: &Ab) -> anyhow::Result<(Config, bool)> {
    let (max_steps, max_time) = s.limits();
    let mut c = Config {
        max_steps,
        max_time,
        approval: s.approval,
        ..Config::default()
    };
    let applied = apply_ab(&mut c, ab)?;
    Ok((c, applied))
}

/// Settings for OpenAI's native computer-use loop (`--backend openai-native`).
/// Needs a direct OpenAI key: the AI Gateway does not serve
/// `computer-use-preview` (404, measured 2026-10-09).
pub fn native_config(
    keys: &Keys,
    model: Option<&str>,
    config: &Config,
) -> anyhow::Result<kai_core::cua::CuaConfig> {
    let key = keys.openai.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "--backend openai-native needs an OpenAI key: run `kai key openai` or set OPENAI_API_KEY"
        )
    })?;
    let mut c = kai_core::cua::CuaConfig {
        api_key: Some(key),
        max_steps: config.max_steps,
        max_time: config.max_time,
        ..Default::default()
    };
    if let Some(m) = model {
        c.model = m.trim_start_matches("openai/").to_string();
    }
    Ok(c)
}

/// One line per key slot for `kai doctor`. Never prints a key.
pub fn key_report() -> Vec<String> {
    use kai_os::secrets as k;
    [
        (k::VLLM, "VLLM_API_KEY", "vllm", "vllm (DigitalOcean)"),
        (k::GEMINI, "GEMINI_API_KEY", "gemini", "gemini"),
        (
            k::GATEWAY,
            "AI_GATEWAY_API_KEY",
            "gateway",
            "gateway (Vercel AI Gateway)",
        ),
        (
            k::OPENAI,
            "OPENAI_API_KEY",
            "openai",
            "openai, openai-native",
        ),
    ]
    .into_iter()
    .map(|(slot, env, which, label)| {
        let from_env = std::env::var(env).is_ok_and(|v| !v.is_empty());
        let state = if from_env {
            format!("key set (from {env})")
        } else if k::get(slot).is_some() {
            "key set (keychain)".to_string()
        } else {
            format!("no key (`kai key {which}` or {env})")
        };
        format!("{label:<30} {state}")
    })
    .collect()
}

/// `kai doctor`'s executor line: is jevos at the settings' endpoint up, and
/// how long does `/health` take (median of 5 round trips)?
pub async fn executor_report(s: &Settings) -> String {
    let c = s.executor_client();
    let mut times = Vec::new();
    for _ in 0..5 {
        match c.health().await {
            Ok(t) => times.push(t.as_micros() as u64),
            Err(e) => {
                return format!(
                    "{:<30} unreachable ({e}); start it with `./jev serve` or set KAI_EXECUTOR_URL",
                    c.base()
                );
            }
        }
    }
    times.sort_unstable();
    let ms = |us: u64| us as f64 / 1000.0;
    format!(
        "{:<30} reachable · /health {:.1} ms median, {:.1} ms max over {} calls",
        c.base(),
        ms(times[times.len() / 2]),
        ms(times[times.len() - 1]),
        times.len()
    )
}

/// A stand-in model client for paths that never call Kai's model (the native
/// OpenAI loop drives itself).
pub fn placeholder_client() -> Box<dyn kai_core::ModelClient> {
    Box::new(kai_core::client::MockClient::always(
        r#"{"thought":"unused","action":"done","risk":"none"}"#,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_defaults_to_gemma_and_model_fills_both_slots() {
        let s = settings_for(BackendArg::Gateway, None).unwrap();
        assert_eq!(s.backend, BackendChoice::Gateway);
        assert_eq!(s.fast_model, GATEWAY_DEFAULT_MODEL);
        let s = settings_for(BackendArg::Gateway, Some("openai/gpt-5-mini")).unwrap();
        assert_eq!(s.fast_model, "openai/gpt-5-mini");
        assert_eq!(s.careful_model, "openai/gpt-5-mini");
        assert!(settings_for(BackendArg::OpenAi, None).is_err());
        let s = settings_for(BackendArg::OpenAi, Some("gpt-5-mini")).unwrap();
        assert_eq!(s.backend, BackendChoice::OpenAi);
        assert_eq!(model_label(BackendArg::Replay, &s), "recorded");
    }

    #[test]
    fn missing_keys_are_named() {
        let keys = Keys::default();
        let e = require_key(BackendArg::Gateway, &keys)
            .unwrap_err()
            .to_string();
        assert!(e.contains("kai key gateway") && e.contains("AI_GATEWAY_API_KEY"));
        assert!(require_key(BackendArg::Mock, &keys).is_ok());
        let c = Config::default();
        let e = native_config(&keys, None, &c).unwrap_err().to_string();
        assert!(e.contains("OPENAI_API_KEY"));
        let keys = Keys {
            openai: Some("x".into()),
            ..Default::default()
        };
        let n = native_config(&keys, Some("openai/computer-use-preview"), &c).unwrap();
        assert_eq!(n.model, "computer-use-preview");
    }
}
