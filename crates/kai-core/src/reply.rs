//! A step reply: one [`Decision`] plus, optionally, a short list of follow-up
//! actions the model is certain about (`then`), a hint that it needs to see
//! the screen next step (`need_screen`), and a short `plan` of upcoming
//! subgoals for the executor (`crate::executor`, `docs/EXECUTOR.md`).
//!
//! Backward compatible: a plain single-object reply (the old format) parses
//! as a reply with no follow-ups. The extra keys are lifted off the JSON
//! object before the rest is parsed as a strict [`Decision`], so the decision
//! itself still rejects unknown fields.

pub use crate::plan::{MAX_PLAN, Subgoal, SubgoalKind};
use crate::schema::{Action, Decision, SchemaError, extract_json_object};
use serde_json::Value;

/// At most this many follow-up actions run without a new model call.
pub const MAX_THEN: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepReply {
    pub decision: Decision,
    /// Follow-up actions, already validated. Empty for a single action.
    pub then: Vec<Decision>,
    /// The model asked for a screenshot on the next step.
    pub need_screen: bool,
    /// Upcoming subgoals for the executor; empty when absent (today's
    /// behaviour). See [`crate::plan::parse_plan`].
    pub plan: Vec<Subgoal>,
    /// Raw `outcomes` (first call only; the engine reads them once and
    /// ignores them on every later reply). See [`crate::ledger`].
    pub outcomes: Vec<Value>,
}

impl StepReply {
    /// Parse and validate. The main decision must be valid (else error, which
    /// triggers the repair retry); an invalid follow-up drops itself and every
    /// follow-up after it, never the main decision.
    pub fn parse_checked(raw: &str) -> Result<Self, SchemaError> {
        let json = extract_json_object(raw)
            .ok_or_else(|| SchemaError::Parse("no JSON object found in reply".to_string()))?;
        let mut v: Value =
            serde_json::from_str(json).map_err(|e| SchemaError::Parse(e.to_string()))?;
        let obj = v
            .as_object_mut()
            .ok_or_else(|| SchemaError::Parse("reply is not a JSON object".to_string()))?;
        let then_raw = obj.remove("then");
        let plan_raw = obj.remove("plan");
        let outcomes = match obj.remove("outcomes") {
            Some(Value::Array(items)) => items,
            _ => Vec::new(),
        };
        let need_screen = obj
            .remove("need_screen")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        let mut decision: Decision =
            serde_json::from_value(v).map_err(|e| SchemaError::Parse(e.to_string()))?;
        decision.validate_and_clamp()?;

        let mut then = Vec::new();
        if !matches!(decision.action, Action::Done | Action::AskUser)
            && let Some(Value::Array(items)) = then_raw
        {
            for item in items.into_iter().take(MAX_THEN) {
                let Some(d) = follow_up(item) else { break };
                // The same click twice in a row toggles a dropdown or menu
                // shut again; it is never a certain follow-up.
                let prev = then.last().unwrap_or(&decision);
                if d.action.needs_target()
                    && d.action != Action::Type
                    && d.action == prev.action
                    && d.target == prev.target
                {
                    break;
                }
                then.push(d);
            }
        }
        let plan = if matches!(decision.action, Action::Done | Action::AskUser) {
            Vec::new()
        } else {
            crate::plan::parse_plan(plan_raw)
        };
        Ok(Self {
            decision,
            then,
            need_screen,
            plan,
            outcomes,
        })
    }

    /// `schema` plus the optional `outcomes` the first call may name.
    pub fn with_outcomes(schema: &Value) -> Value {
        let mut s = schema.clone();
        s["properties"]["outcomes"] = crate::ledger::outcomes_schema();
        s
    }

    /// [`StepReply::json_schema`] plus the optional `plan` (executor on).
    pub fn json_schema_with_plan() -> Value {
        let mut s = Self::json_schema();
        s["properties"]["plan"] = crate::plan::plan_schema();
        s
    }

    /// The JSON schema for structured outputs: the [`Decision`] schema plus
    /// optional `then` (max [`MAX_THEN`] actions) and `need_screen`.
    pub fn json_schema() -> Value {
        let mut s = Decision::json_schema();
        // A lean item: what batched actions need (no scroll, no thought).
        let action = s["properties"]["action"].clone();
        s["properties"]["then"] = serde_json::json!({
            "type": "array",
            "maxItems": MAX_THEN,
            "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["action"],
                "properties": {
                    "action": action,
                    "target": { "type": ["integer", "null"] },
                    "text": { "type": ["string", "null"] },
                    "keys": { "type": "array", "items": { "type": "string" } },
                    "risk": { "type": "string", "enum": ["none","low","high"] },
                    "expect": { "type": "string" }
                }
            }
        });
        s["properties"]["need_screen"] = serde_json::json!({ "type": "boolean" });
        s
    }
}

/// One follow-up; `thought` is optional there. Terminal actions (`done`,
/// `ask_user`) never run unverified, so they end the batch.
fn follow_up(mut item: Value) -> Option<Decision> {
    let o = item.as_object_mut()?;
    o.entry("thought")
        .or_insert_with(|| Value::String(String::new()));
    o.remove("then");
    o.remove("need_screen");
    let mut d: Decision = serde_json::from_value(item).ok()?;
    d.validate_and_clamp().ok()?;
    if matches!(d.action, Action::Done | Action::AskUser) {
        return None;
    }
    Some(d)
}

#[cfg(test)]
mod tests {

    #[test]
    fn repeated_click_ends_the_batch() {
        let r = StepReply::parse_checked(
            r#"{"thought":"x","action":"click","target":34,"risk":"none",
                "then":[{"action":"click","target":34},{"action":"key","keys":["a"]}]}"#,
        )
        .unwrap();
        assert!(r.then.is_empty());
    }

    use super::*;

    #[test]
    fn old_single_object_still_parses() {
        let r = StepReply::parse_checked(
            r#"{"thought":"save","action":"click","target":3,"risk":"low","expect":"saved"}"#,
        )
        .unwrap();
        assert_eq!(r.decision.target, Some(3));
        assert!(r.then.is_empty());
        assert!(!r.need_screen);
    }

    #[test]
    fn strict_mode_reply_with_every_field_parses() {
        // OpenAI strict structured outputs emit every property, nulls included.
        let r = StepReply::parse_checked(
            r#"{"thought":"search","action":"type","target":4,"text":"lofi","keys":[],
                "direction":null,"amount":null,"risk":"none","expect":"typed",
                "then":[{"action":"key","target":null,"text":null,"keys":["Return"],
                         "risk":"none","expect":"results"}],
                "need_screen":false}"#,
        )
        .unwrap();
        assert_eq!(r.decision.target, Some(4));
        assert_eq!(r.then.len(), 1);
        assert_eq!(r.then[0].target, None);
    }

    #[test]
    fn follow_ups_parse_without_thought() {
        let r = StepReply::parse_checked(
            r#"{"thought":"search","action":"type","target":2,"text":"gemma","risk":"none",
               "then":[{"action":"key","keys":["Return"]}],"need_screen":true}"#,
        )
        .unwrap();
        assert_eq!(r.then.len(), 1);
        assert_eq!(r.then[0].keys, vec!["enter"]);
        assert!(r.need_screen);
    }

    #[test]
    fn invalid_follow_up_truncates_the_batch_only() {
        let r = StepReply::parse_checked(
            r#"{"thought":"x","action":"open_url","text":"https://a.b","risk":"none",
               "then":[{"action":"key","keys":["ctrl","l"]},{"action":"click"},{"action":"key","keys":["enter"]}]}"#,
        )
        .unwrap();
        assert_eq!(r.then.len(), 1, "click without target ends the batch");
    }

    #[test]
    fn batch_is_capped_and_terminal_actions_end_it() {
        let k = r#"{"action":"key","keys":["tab"]}"#;
        let raw = format!(
            r#"{{"thought":"x","action":"key","keys":["tab"],"risk":"none","then":[{k},{k},{k},{k},{k}]}}"#
        );
        assert_eq!(StepReply::parse_checked(&raw).unwrap().then.len(), MAX_THEN);
        let r = StepReply::parse_checked(
            r#"{"thought":"x","action":"key","keys":["tab"],"risk":"none","then":[{"action":"done"},{"action":"key","keys":["a"]}]}"#,
        )
        .unwrap();
        assert!(r.then.is_empty());
    }

    #[test]
    fn done_ignores_then_and_unknown_fields_still_fail() {
        let r = StepReply::parse_checked(
            r#"{"thought":"x","action":"done","risk":"none","then":[{"action":"key","keys":["a"]}]}"#,
        )
        .unwrap();
        assert!(r.then.is_empty());
        assert!(
            StepReply::parse_checked(r#"{"thought":"x","action":"done","risk":"none","evil":1}"#)
                .is_err()
        );
    }

    #[test]
    fn plan_is_optional_and_ignored_on_done() {
        let r = StepReply::parse_checked(
            r#"{"thought":"x","action":"wait","risk":"none",
               "plan":[{"do":"click","target":"Send","expect":"sent"}]}"#,
        )
        .unwrap();
        assert_eq!(r.plan.len(), 1);
        let r = StepReply::parse_checked(
            r#"{"thought":"x","action":"done","risk":"none","plan":[{"do":"click","target":"A"}]}"#,
        )
        .unwrap();
        assert!(r.plan.is_empty());
        let s = StepReply::json_schema_with_plan();
        assert_eq!(s["properties"]["plan"]["maxItems"], MAX_PLAN);
        assert!(StepReply::json_schema()["properties"].get("plan").is_none());
    }

    #[test]
    fn schema_has_then_and_need_screen() {
        let s = StepReply::json_schema();
        assert_eq!(s["properties"]["then"]["maxItems"], MAX_THEN);
        assert!(
            s["properties"]["then"]["items"]["properties"]
                .get("thought")
                .is_none()
        );
        assert_eq!(s["properties"]["need_screen"]["type"], "boolean");
        eprintln!(
            "MEASURED schema JSON: decision {} chars, step reply {} chars",
            Decision::json_schema().to_string().len(),
            s.to_string().len()
        );
    }
}
