//! The model's short plan of upcoming subgoals (`plan` in a step reply). Each
//! subgoal describes its element in plain words (`target`), never by number,
//! because the screen will have changed by the time it runs. The executor
//! (`crate::executor`) grounds, gates and runs them; see `docs/EXECUTOR.md`.

use crate::schema::{Action, Decision, Risk, SchemaError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// At most this many planned subgoals per reply.
pub const MAX_PLAN: usize = 6;

/// What a planned subgoal does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubgoalKind {
    Click,
    Type,
    Key,
    Select,
    OpenUrl,
    OpenApp,
    Shell,
    Done,
}

impl SubgoalKind {
    /// Kinds that point at an element the executor must find.
    pub fn needs_target(self) -> bool {
        matches!(
            self,
            SubgoalKind::Click | SubgoalKind::Type | SubgoalKind::Select
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            SubgoalKind::Click => "click",
            SubgoalKind::Type => "type",
            SubgoalKind::Key => "key",
            SubgoalKind::Select => "select",
            SubgoalKind::OpenUrl => "open_url",
            SubgoalKind::OpenApp => "open_app",
            SubgoalKind::Shell => "shell",
            SubgoalKind::Done => "done",
        }
    }
}

/// One upcoming step in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subgoal {
    #[serde(rename = "do")]
    pub kind: SubgoalKind,
    #[serde(default)]
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    #[serde(default)]
    pub expect: String,
    /// The model's own risk claim (Kai's rules may raise it, never lower).
    #[serde(default)]
    pub risk: Risk,
}

impl Subgoal {
    /// "type 'ada@x.org' into Email field", for notes and the trace.
    pub fn describe(&self) -> String {
        let mut s = self.kind.label().to_string();
        if let Some(t) = self.text.as_deref().filter(|t| !t.is_empty()) {
            let short: String = t.chars().take(40).collect();
            s.push_str(&format!(" '{short}'"));
        }
        if !self.keys.is_empty() {
            s.push_str(&format!(" {}", self.keys.join("+")));
        }
        if !self.target.trim().is_empty() {
            s.push_str(if self.kind == SubgoalKind::Type {
                " into "
            } else {
                " "
            });
            s.push_str(self.target.trim());
        }
        s
    }

    /// The validated action for this subgoal acting on mark `target`
    /// (ignored by kinds that point at nothing). `select` types the option
    /// into a dropdown when it has `text` (the engine commits it with Enter),
    /// else clicks the item.
    pub fn to_decision(&self, target: Option<u32>) -> Result<Decision, SchemaError> {
        let has_text = self.text.as_deref().is_some_and(|t| !t.is_empty());
        let action = match self.kind {
            SubgoalKind::Click => Action::Click,
            SubgoalKind::Type => Action::Type,
            SubgoalKind::Select if has_text => Action::Type,
            SubgoalKind::Select => Action::Click,
            SubgoalKind::Key => Action::Key,
            SubgoalKind::OpenUrl => Action::OpenUrl,
            SubgoalKind::OpenApp => Action::OpenApp,
            SubgoalKind::Shell => Action::Shell,
            SubgoalKind::Done => Action::Done,
        };
        let mut d = Decision {
            thought: format!("executor: {}", self.describe()),
            action,
            target: if action.needs_target() { target } else { None },
            text: self.text.clone().filter(|_| action != Action::Click),
            keys: if action == Action::Key {
                self.keys.clone()
            } else {
                Vec::new()
            },
            direction: None,
            amount: None,
            risk: self.risk,
            expect: self.expect.clone(),
        };
        d.validate_and_clamp()?;
        Ok(d)
    }

    fn valid(&self) -> bool {
        (!self.kind.needs_target() || !self.target.trim().is_empty())
            && self.to_decision(Some(1)).is_ok()
    }
}

/// Parse a raw `plan` array: at most [`MAX_PLAN`] items; the first invalid
/// item ends the plan; `done` is always last.
pub fn parse_plan(raw: Option<Value>) -> Vec<Subgoal> {
    let mut plan = Vec::new();
    let Some(Value::Array(items)) = raw else {
        return plan;
    };
    for item in items.into_iter().take(MAX_PLAN) {
        let Ok(sg) = serde_json::from_value::<Subgoal>(item) else {
            break;
        };
        if !sg.valid() {
            break;
        }
        let done = sg.kind == SubgoalKind::Done;
        plan.push(sg);
        if done {
            break;
        }
    }
    plan
}

/// The JSON schema of the `plan` property.
pub fn plan_schema() -> Value {
    serde_json::json!({
        "type": "array",
        "maxItems": MAX_PLAN,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["do", "target", "expect"],
            "properties": {
                "do": { "type": "string", "enum":
                    ["click","type","key","select","open_url","open_app","shell","done"] },
                "target": { "type": "string" },
                "text": { "type": ["string", "null"] },
                "keys": { "type": "array", "items": { "type": "string" } },
                "expect": { "type": "string" },
                "risk": { "type": "string", "enum": ["none","low","high"] }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plan_parses_validates_and_stops_at_done() {
        let plan = parse_plan(Some(json!([
            {"do":"type","target":"Email field","text":"a@b.c","expect":"email typed"},
            {"do":"key","target":"","keys":["Return"],"expect":"sent"},
            {"do":"done","target":"","expect":""},
            {"do":"click","target":"After done","expect":""}
        ])));
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].describe(), "type 'a@b.c' into Email field");
        assert_eq!(plan[1].to_decision(None).unwrap().keys, vec!["enter"]);
        // A type subgoal without text ends the plan; so does a click with no target.
        let plan = parse_plan(Some(json!([
            {"do":"click","target":"Send","expect":"sent"},
            {"do":"type","target":"Name"},
            {"do":"click","target":"Ok"}
        ])));
        assert_eq!(plan.len(), 1);
        assert!(parse_plan(Some(json!([{"do":"click","target":"  "}]))).is_empty());
        assert!(parse_plan(Some(json!([{"do":"fly","target":"x"}]))).is_empty());
        assert!(parse_plan(None).is_empty());
        let many: Vec<_> = (0..9)
            .map(|_| json!({"do":"key","keys":["tab"],"expect":""}))
            .collect();
        assert_eq!(parse_plan(Some(Value::Array(many))).len(), MAX_PLAN);
    }

    #[test]
    fn select_types_into_a_dropdown_or_clicks_an_item() {
        let mut sg: Subgoal =
            serde_json::from_value(json!({"do":"select","target":"Country","text":"Italy"}))
                .unwrap();
        assert_eq!(sg.to_decision(Some(4)).unwrap().action, Action::Type);
        sg.text = None;
        let d = sg.to_decision(Some(4)).unwrap();
        assert_eq!((d.action, d.target), (Action::Click, Some(4)));
    }
}
