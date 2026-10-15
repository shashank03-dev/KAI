//! The one thing the model returns each step: a validated [`Decision`].
//!
//! The model is never trusted. We parse with `serde` (rejecting unknown
//! fields), then [`Decision::validate_and_clamp`] forces every field into a
//! sane range before the actuator ever sees it.

use serde::{Deserialize, Serialize};

/// What Kai does in one step. One action per step, always.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Click,
    DoubleClick,
    RightClick,
    Type,
    Key,
    Scroll,
    OpenApp,
    OpenUrl,
    Wait,
    AskUser,
    Done,
    /// Run a terminal command; `text` holds it. Output is fed back next step.
    Shell,
}

impl Action {
    /// Actions that point at a Set-of-Marks element and therefore need `target`.
    pub fn needs_target(self) -> bool {
        matches!(
            self,
            Action::Click | Action::DoubleClick | Action::RightClick | Action::Type
        )
    }

    /// Actions that reach the OS directly and skip grounding (fast paths).
    pub fn is_fast_path(self) -> bool {
        matches!(self, Action::OpenApp | Action::OpenUrl)
    }
}

/// How dangerous an action is. A password field forces [`Risk::High`] no matter
/// what the model claims (see `risk.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    #[default]
    None,
    Low,
    High,
}

/// Scroll direction for [`Action::Scroll`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Down,
    Up,
    Left,
    Right,
}

/// A single model decision. Mirrors the schema in the build prompt exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    /// Short reasoning, capped at 25 words after clamping.
    pub thought: String,
    pub action: Action,
    /// Set-of-Marks element number, when the action needs one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<u32>,
    #[serde(default)]
    pub risk: Risk,
    /// What the screen should show after this action; checked next observation.
    #[serde(default)]
    pub expect: String,
}

/// Why a reply was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SchemaError {
    #[error("reply was not valid JSON for the action schema: {0}")]
    Parse(String),
    #[error("action `{0:?}` requires a `target` element number")]
    MissingTarget(Action),
    #[error("action `{0:?}` requires non-empty `text`")]
    MissingText(Action),
    #[error("action `{0:?}` requires at least one key in `keys`")]
    MissingKeys(Action),
    #[error("action `open_url` requires a URL in `text`")]
    MissingUrl,
}

const MAX_THOUGHT_WORDS: usize = 25;
const MAX_SCROLL_AMOUNT: u32 = 20;
const MAX_TEXT_LEN: usize = 8192;

impl Decision {
    /// Parse a raw model reply as a [`Decision`]. Tolerates a leading/trailing
    /// code fence or prose around the JSON object (some backends add it).
    pub fn parse(raw: &str) -> Result<Self, SchemaError> {
        let json = extract_json_object(raw)
            .ok_or_else(|| SchemaError::Parse("no JSON object found in reply".to_string()))?;
        serde_json::from_str::<Decision>(json).map_err(|e| SchemaError::Parse(e.to_string()))
    }

    /// Parse then validate-and-clamp in one step.
    pub fn parse_checked(raw: &str) -> Result<Self, SchemaError> {
        let mut d = Self::parse(raw)?;
        d.validate_and_clamp()?;
        Ok(d)
    }

    /// Force every field into range. Required fields missing for the action are
    /// an error; everything else is clamped, never rejected.
    pub fn validate_and_clamp(&mut self) -> Result<(), SchemaError> {
        // Clamp thought to 25 words.
        let words: Vec<&str> = self.thought.split_whitespace().collect();
        if words.len() > MAX_THOUGHT_WORDS {
            self.thought = words[..MAX_THOUGHT_WORDS].join(" ");
        }

        if let Some(t) = &mut self.text
            && t.len() > MAX_TEXT_LEN
        {
            t.truncate(t.floor_char_boundary(MAX_TEXT_LEN));
        }

        if let Some(a) = self.amount {
            self.amount = Some(a.clamp(1, MAX_SCROLL_AMOUNT));
        }

        // Normalise keys: trim, drop empties, lowercase the modifier names.
        self.keys.retain(|k| !k.trim().is_empty());
        for k in &mut self.keys {
            *k = normalise_key(k);
        }

        // Per-action required fields.
        match self.action {
            a if a.needs_target() && self.target.is_none() => {
                return Err(SchemaError::MissingTarget(a));
            }
            Action::Type if self.text.as_deref().unwrap_or("").is_empty() => {
                return Err(SchemaError::MissingText(Action::Type));
            }
            Action::Shell if self.text.as_deref().unwrap_or("").trim().is_empty() => {
                return Err(SchemaError::MissingText(Action::Shell));
            }
            Action::Key if self.keys.is_empty() => {
                return Err(SchemaError::MissingKeys(Action::Key));
            }
            Action::OpenApp if self.text.as_deref().unwrap_or("").trim().is_empty() => {
                return Err(SchemaError::MissingText(Action::OpenApp));
            }
            Action::OpenUrl if self.text.as_deref().unwrap_or("").is_empty() => {
                return Err(SchemaError::MissingUrl);
            }
            Action::Scroll => {
                self.direction.get_or_insert(Direction::Down);
                self.amount.get_or_insert(3);
            }
            _ => {}
        }
        Ok(())
    }

    /// The JSON schema sent to vLLM structured outputs, so the reply is always
    /// valid on that backend. Kept in sync with the fields above by a test.
    pub fn json_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["thought", "action", "risk"],
            "properties": {
                "thought": { "type": "string" },
                "action": { "type": "string", "enum": [
                    "click","double_click","right_click","type","key","scroll",
                    "open_app","open_url","wait","ask_user","done","shell"] },
                "target": { "type": ["integer", "null"], "minimum": 0 },
                "text": { "type": ["string", "null"] },
                "keys": { "type": "array", "items": { "type": "string" } },
                "direction": { "type": ["string", "null"],
                    "enum": ["down","up","left","right", null] },
                "amount": { "type": ["integer", "null"], "minimum": 1 },
                "risk": { "type": "string", "enum": ["none","low","high"] },
                "expect": { "type": "string" }
            }
        })
    }
}

fn normalise_key(k: &str) -> String {
    let lower = k.trim().to_ascii_lowercase();
    match lower.as_str() {
        "command" | "cmd" | "meta" | "super" | "win" => "cmd".to_string(),
        "control" | "ctrl" => "ctrl".to_string(),
        "option" | "alt" => "alt".to_string(),
        "shift" => "shift".to_string(),
        "return" | "enter" => "enter".to_string(),
        "escape" | "esc" => "esc".to_string(),
        other => other.to_string(),
    }
}

/// Find the first balanced `{...}` JSON object in a string, ignoring braces that
/// appear inside string literals. Robust against code fences and prose.
pub(crate) fn extract_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = s.find('{')?;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for i in start..bytes.len() {
        let c = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_clean_click() {
        let d = Decision::parse_checked(
            r#"{"thought":"press save","action":"click","target":14,"risk":"low","expect":"saved"}"#,
        )
        .unwrap();
        assert_eq!(d.action, Action::Click);
        assert_eq!(d.target, Some(14));
        assert_eq!(d.risk, Risk::Low);
    }

    #[test]
    fn tolerates_code_fence_and_prose() {
        let raw =
            "Sure!\n```json\n{\"thought\":\"go\",\"action\":\"done\",\"risk\":\"none\"}\n```\n";
        let d = Decision::parse_checked(raw).unwrap();
        assert_eq!(d.action, Action::Done);
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = Decision::parse(r#"{"thought":"x","action":"done","risk":"none","evil":1}"#)
            .unwrap_err();
        assert!(matches!(err, SchemaError::Parse(_)));
    }

    #[test]
    fn click_without_target_is_rejected() {
        let err = Decision::parse_checked(r#"{"thought":"x","action":"click","risk":"none"}"#)
            .unwrap_err();
        assert_eq!(err, SchemaError::MissingTarget(Action::Click));
    }

    #[test]
    fn type_needs_text() {
        let err =
            Decision::parse_checked(r#"{"thought":"x","action":"type","target":2,"risk":"none"}"#)
                .unwrap_err();
        assert_eq!(err, SchemaError::MissingText(Action::Type));
    }

    #[test]
    fn thought_is_clamped_to_25_words() {
        let long = (0..40).map(|i| i.to_string()).collect::<Vec<_>>().join(" ");
        let raw = format!(r#"{{"thought":"{long}","action":"done","risk":"none"}}"#);
        let d = Decision::parse_checked(&raw).unwrap();
        assert_eq!(d.thought.split_whitespace().count(), MAX_THOUGHT_WORDS);
    }

    #[test]
    fn unicode_text_crossing_byte_limit_is_safely_bounded() {
        for suffix in ["é", "界", "🦀"] {
            for room in 1..suffix.len() {
                let text = "a".repeat(8192 - room) + suffix;
                let raw = serde_json::json!({
                    "thought": "type", "action": "type", "target": 1,
                    "text": text, "risk": "none"
                });
                let d = Decision::parse_checked(&raw.to_string()).unwrap();
                assert_eq!(d.text.unwrap(), "a".repeat(8192 - room));
            }
        }
    }

    #[test]
    fn scroll_amount_is_clamped_and_defaulted() {
        let d = Decision::parse_checked(
            r#"{"thought":"x","action":"scroll","amount":999,"risk":"none"}"#,
        )
        .unwrap();
        assert_eq!(d.amount, Some(MAX_SCROLL_AMOUNT));
        assert_eq!(d.direction, Some(Direction::Down));
    }

    #[test]
    fn keys_are_normalised() {
        let d = Decision::parse_checked(
            r#"{"thought":"x","action":"key","keys":["Command"," S "],"risk":"none"}"#,
        )
        .unwrap();
        assert_eq!(d.keys, vec!["cmd", "s"]);
    }

    #[test]
    fn adversarial_garbage_is_rejected_not_panicking() {
        for raw in [
            "",
            "not json at all",
            "{",
            "{\"thought\": \"unterminated",
            "{\"action\":\"fly\"}",
            "[]",
        ] {
            assert!(
                Decision::parse_checked(raw).is_err(),
                "should reject: {raw:?}"
            );
        }
    }

    #[test]
    fn schema_enumerates_every_action() {
        let schema = Decision::json_schema();
        let enumv = schema["properties"]["action"]["enum"].as_array().unwrap();
        // One entry per Action variant.
        assert_eq!(enumv.len(), 12);
    }
}
