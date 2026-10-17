//! Which model IDs each provider can run, and where a listed ID came from.
//! A model appearing in a catalog never proves account access or screenshot
//! support: only a real request does, and Kai never sends one unasked.

use crate::settings::{BackendChoice, MODELS, ModelRole, Settings};
use serde::{Deserialize, Serialize};

/// Where a catalog entry came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelOrigin {
    /// Returned by the provider's own catalog for this account or server.
    #[default]
    Discovered,
    /// A name the provider documents; Kai did not fetch it.
    Documented,
    /// An exact ID the person typed; no catalog listed it.
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteModel {
    /// The exact ID sent to the provider, never rewritten.
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub origin: ModelOrigin,
    /// What the catalog advertises about image input, when it says anything.
    #[serde(default)]
    pub image_input: Option<bool>,
}

impl RemoteModel {
    pub fn documented(id: &str, label: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            origin: ModelOrigin::Documented,
            image_input: None,
        }
    }

    /// The picker text: the label, the exact ID when it differs, and how
    /// much is known about the entry.
    pub fn describe(&self) -> String {
        let mut text = self.label.clone();
        if self.label != self.id {
            text.push_str(&format!(" · {}", self.id));
        }
        text.push_str(match self.origin {
            ModelOrigin::Discovered => " · discovered",
            ModelOrigin::Documented => " · documented alias",
            ModelOrigin::Manual => " · entered by hand, not in this catalog",
        });
        match self.image_input {
            Some(true) => text.push_str(" · advertises image input"),
            Some(false) => text.push_str(" · no image input advertised"),
            None => {}
        }
        text
    }
}

/// Claude Code's documented model aliases (`claude --model <alias>`).
pub const CLAUDE_ALIASES: &[&str] = &["sonnet", "opus", "haiku"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Gemma,
    Gemini,
    Claude,
    Other,
}

fn family(id: &str) -> Family {
    let id = id.to_ascii_lowercase();
    let bare = id.rsplit('/').next().unwrap_or(&id);
    if bare.starts_with("gemma") {
        Family::Gemma
    } else if bare.starts_with("gemini") {
        Family::Gemini
    } else if bare.starts_with("claude") || CLAUDE_ALIASES.contains(&bare) {
        Family::Claude
    } else {
        Family::Other
    }
}

/// Why `id` cannot run on `backend`, in words for the person; Ok when it can.
pub fn check(backend: BackendChoice, id: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("No model is selected. Refresh the provider's models and pick one.".into());
    }
    if id.trim() != id || id.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("\"{id}\" is not a model ID: IDs have no spaces."));
    }
    let f = family(id);
    let fits = match backend {
        BackendChoice::Claude => CLAUDE_ALIASES.contains(&id) || id.starts_with("claude-"),
        BackendChoice::Codex => f == Family::Other && !id.contains('/'),
        BackendChoice::OpenAi => {
            f == Family::Other && (!id.contains('/') || id.starts_with("openai/"))
        }
        BackendChoice::Gemini => matches!(f, Family::Gemma | Family::Gemini),
        // Droplet, then Gemini, then the Gateway: bare Claude aliases fit none.
        BackendChoice::Both => !CLAUDE_ALIASES.contains(&id),
        BackendChoice::Vllm | BackendChoice::Gateway | BackendChoice::Local => true,
        BackendChoice::Mock => true,
    };
    if fits {
        return Ok(());
    }
    Err(match backend {
        BackendChoice::Claude => format!(
            "Claude Code runs Claude models only. \"{id}\" is not one: pick sonnet, opus, haiku or a claude-… ID."
        ),
        BackendChoice::Codex => format!(
            "Codex runs OpenAI models from its own catalog. \"{id}\" is not one: refresh and pick a listed model."
        ),
        BackendChoice::OpenAi => format!("The OpenAI API does not serve \"{id}\"."),
        BackendChoice::Gemini => {
            format!("The Gemini API serves Gemma and Gemini models only, not \"{id}\".")
        }
        _ => format!("\"{id}\" does not run on this backend."),
    })
}

/// Both model slots of `settings` fit its backend. Local runs use the local
/// model instead of these slots.
pub fn check_settings(settings: &Settings) -> Result<(), String> {
    if settings.backend == BackendChoice::Local {
        return Ok(());
    }
    check(settings.backend, &settings.fast_model)?;
    check(settings.backend, &settings.careful_model).map_err(|e| format!("Careful model: {e}"))
}

/// The model a backend starts with when the previous one does not fit it.
/// Codex has none: its models come only from the signed-in account.
pub fn default_model(backend: BackendChoice) -> &'static str {
    match backend {
        BackendChoice::Claude => CLAUDE_ALIASES[0],
        BackendChoice::Codex => "",
        BackendChoice::OpenAi => "openai/gpt-5-mini",
        _ => MODELS
            .iter()
            .find(|m| m.role == ModelRole::Fast)
            .map(|m| m.id)
            .unwrap_or_default(),
    }
}

/// What the picker offers before (or without) a fetched catalog: the names
/// Kai documents for the backend, marked as such.
pub fn documented(backend: BackendChoice) -> Vec<RemoteModel> {
    match backend {
        BackendChoice::Claude => CLAUDE_ALIASES
            .iter()
            .map(|id| RemoteModel::documented(id, &format!("Claude {id}")))
            .collect(),
        BackendChoice::Codex | BackendChoice::Local | BackendChoice::Mock => vec![],
        _ => MODELS
            .iter()
            .filter(|m| check(backend, m.id).is_ok())
            .map(|m| RemoteModel::documented(m.id, m.label))
            .collect(),
    }
}

/// The picker's entries: the catalog, plus the saved selection when it fits
/// the backend but is not listed (an ID typed by hand), so the picker never
/// hides what Kai will actually send.
pub fn choices(
    backend: BackendChoice,
    catalog: &[RemoteModel],
    selected: &str,
) -> Vec<RemoteModel> {
    let mut out = catalog.to_vec();
    if !catalog.iter().any(|m| m.id == selected) && check(backend, selected).is_ok() {
        out.push(RemoteModel {
            id: selected.into(),
            label: selected.into(),
            origin: ModelOrigin::Manual,
            image_input: None,
        });
    }
    out
}
