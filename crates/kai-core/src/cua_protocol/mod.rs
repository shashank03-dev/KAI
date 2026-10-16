//! Pure adapter for OpenAI's Responses `computer` tool (GA `actions[]`) and
//! the preview's single `action`. No network or execution: callers authorize
//! and dispatch the results. The exact wire shapes, including the nullable
//! fields real responses carry, are in `docs/RESEARCH_2026_10_OPENAI_PARITY.md`
//! §3.
mod action;
mod output;
pub use action::parse_action;
pub use output::{harness_note, screenshot_output};

use crate::computer::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A `pending_safety_checks` entry. Echoed verbatim, once the user agreed,
/// as `acknowledged_safety_checks`; the API refuses the next call otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyCheck {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug)]
pub struct ComputerCall {
    pub call_id: String,
    pub actions: Vec<ComputerAction>,
    /// Checks the user must accept before any of `actions` runs.
    pub pending: Vec<SafetyCheck>,
}

/// A message's role in the turn: `commentary` keeps the run going,
/// `final_answer` (or a message with no call) ends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Commentary,
    FinalAnswer,
    Unknown,
}

#[derive(Debug)]
pub struct ComputerResponse {
    pub calls: Vec<ComputerCall>,
    pub message: Option<(String, Phase)>,
}

/// A field that is absent, `null` or `[]` means "none" (real GA responses
/// carry `"action": null` and `"pending_safety_checks": null`).
pub(crate) fn present(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
}

fn safety_checks(item: &Value) -> Result<Vec<SafetyCheck>, ToolError> {
    match present(item.get("pending_safety_checks")) {
        None => Ok(Vec::new()),
        Some(v) => {
            let checks: Vec<SafetyCheck> =
                serde_json::from_value(v.clone()).map_err(|_| ToolError::InvalidArguments)?;
            if checks.iter().any(|c| c.id.is_empty()) {
                return Err(ToolError::InvalidArguments);
            }
            Ok(checks)
        }
    }
}

fn computer_call(
    item: &Value,
    observation: u64,
    focused: Option<u32>,
) -> Result<ComputerCall, ToolError> {
    if item.get("status").is_some_and(|s| s != "completed") {
        return Err(ToolError::Failed);
    }
    let call_id = item["call_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(ToolError::InvalidArguments)?
        .to_string();
    // Exactly one of the preview's `action` and GA's `actions` is set.
    let raw: Vec<&Value> = match (present(item.get("action")), present(item.get("actions"))) {
        (Some(a), None) => vec![a],
        (None, Some(a)) => a
            .as_array()
            .ok_or(ToolError::InvalidArguments)?
            .iter()
            .collect(),
        _ => return Err(ToolError::InvalidArguments),
    };
    if raw.len() > MAX_BATCH {
        return Err(ToolError::InvalidArguments);
    }
    let actions = raw
        .into_iter()
        .map(|a| parse_action(a, observation, focused))
        .collect::<Result<_, _>>()?;
    Ok(ComputerCall {
        call_id,
        actions,
        pending: safety_checks(item)?,
    })
}

fn phase(item: &Value) -> Phase {
    match item["phase"].as_str() {
        Some("commentary") => Phase::Commentary,
        Some("final_answer") => Phase::FinalAnswer,
        _ => Phase::Unknown,
    }
}

pub fn parse_response(
    v: &Value,
    observation: u64,
    focused: Option<u32>,
) -> Result<ComputerResponse, ToolError> {
    if v.get("status").is_some_and(|s| s != "completed")
        || v.get("error").is_some_and(|e| !e.is_null())
    {
        return Err(ToolError::Failed);
    }
    let output = v["output"].as_array().ok_or(ToolError::InvalidArguments)?;
    let mut calls: Vec<ComputerCall> = Vec::new();
    let mut message = None;
    for item in output {
        match item["type"].as_str() {
            Some("computer_call") => {
                let call = computer_call(item, observation, focused)?;
                if calls.iter().any(|c| c.call_id == call.call_id) {
                    return Err(ToolError::InvalidArguments);
                }
                calls.push(call);
            }
            Some("message") => {
                let text = item["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|c| c["type"] == "output_text")
                    .filter_map(|c| c["text"].as_str())
                    .next_back();
                if let Some(text) = text {
                    message = Some((text.to_string(), phase(item)));
                }
            }
            Some("reasoning") => {}
            _ => return Err(ToolError::Unsupported),
        }
    }
    if calls.iter().map(|c| c.actions.len()).sum::<usize>() > MAX_BATCH {
        return Err(ToolError::InvalidArguments);
    }
    Ok(ComputerResponse { calls, message })
}
