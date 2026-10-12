//! The outcome ledger (`docs/VERIFICATION.md`): the 1–4 outcomes the model
//! said must hold when the task is done, each with an optional probe from a
//! fixed menu. Every outcome starts `pending`; only probe results (and the
//! user waiving the gate) change its status. The model's text, its later
//! replies and memory never do: the parser has no status field at all.

use crate::human::{OneTimeCode, REDACTED_CODE};
use crate::ports::Observation;
use crate::probe::{ProbeSpec, ProbeVerdict, RawProbe, clip};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// At most this many required outcomes per run.
pub const MAX_OUTCOMES: usize = 4;
const MAX_ID: usize = 24;
const MAX_DESCRIPTION: usize = 120;

/// Where an outcome stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Pending,
    Verified,
    /// Verified earlier, but its probe failed when `done` was claimed.
    Reverted,
}

/// Who verified an outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifiedBy {
    Probe,
    /// The user pressed "I did it" when Kai could not confirm (the gate was
    /// waived; the outcome itself stays unverified).
    User,
}

/// What an action did, judged from the tree diff, accessibility events and
/// the screen hash, with no model call (stage 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Confirmed,
    Unverifiable,
    NoOp,
}

/// One required outcome, as recorded in `trace.outcomes` and shown in the
/// UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutcomeRecord {
    pub id: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<String>,
    /// Why the model's probe was not accepted (the outcome stays, probe-less).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_refused: Option<String>,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<VerifiedBy>,
    /// Steps whose action was `confirmed` before this outcome was verified.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<u32>,
}

/// One failed probe behind a gate refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateFail {
    pub outcome: String,
    pub probe: String,
    pub reason: String,
}

/// One `trace.gate` entry: a refused `done`, or the user waiving the gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateRecord {
    pub step: u32,
    /// `refused` or `waived_by_user`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<GateFail>,
    /// The effect of the action just before the `done` claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_effect: Option<Effect>,
}

/// One outcome in the live ledger.
#[derive(Debug, Clone)]
struct Required {
    rec: OutcomeRecord,
    probe: Option<ProbeSpec>,
}

/// The run's required outcomes.
#[derive(Debug, Clone, Default)]
pub struct Ledger {
    items: Vec<Required>,
    /// Confirmed steps not yet credited to an outcome.
    confirmed: Vec<u32>,
}

/// An outcome as the model writes it. Unknown keys (a `status` included)
/// are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawOutcome {
    id: Option<String>,
    description: Option<String>,
}

fn clean_id(raw: Option<&str>, taken: &[String], n: usize) -> String {
    let id: String = raw
        .unwrap_or("")
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(MAX_ID)
        .collect();
    if id.is_empty() || taken.contains(&id) {
        let mut k = n;
        loop {
            let id = format!("o{k}");
            if !taken.contains(&id) {
                return id;
            }
            k += 1;
        }
    }
    id
}

impl Ledger {
    /// Build the ledger from the first reply's raw `outcomes`. A bad probe
    /// never drops its outcome: the outcome is kept without a probe and the
    /// reason is recorded, so a refused probe cannot switch the gate off.
    pub fn from_raw(raw: &[Value]) -> Self {
        let mut items: Vec<Required> = Vec::new();
        for v in raw.iter().take(MAX_OUTCOMES) {
            let Some(obj) = v.as_object() else { continue };
            let o: RawOutcome = serde_json::from_value(v.clone()).unwrap_or_default();
            let (probe, refused) = match obj.get("probe") {
                None | Some(Value::Null) => (None, None),
                Some(p) => match serde_json::from_value::<RawProbe>(p.clone()) {
                    Ok(rp) => match ProbeSpec::parse(&rp) {
                        Ok(spec) => (Some(spec), None),
                        Err(e) => (None, Some(e)),
                    },
                    Err(_) => (None, Some("probe is not an object".into())),
                },
            };
            let mut description = clip(o.description.as_deref().unwrap_or(""), MAX_DESCRIPTION);
            if description.is_empty() {
                match &probe {
                    Some(p) => description = p.label(),
                    None => continue,
                }
            }
            let taken: Vec<String> = items.iter().map(|i| i.rec.id.clone()).collect();
            let id = clean_id(o.id.as_deref(), &taken, items.len() + 1);
            items.push(Required {
                rec: OutcomeRecord {
                    id,
                    description,
                    probe: probe.as_ref().map(ProbeSpec::label),
                    probe_refused: refused,
                    status: Status::Pending,
                    verified_at: None,
                    by: None,
                    evidence: Vec::new(),
                },
                probe,
            });
        }
        Self {
            items,
            confirmed: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// At least one outcome has a probe: `done` is gated.
    pub fn gated(&self) -> bool {
        self.items.iter().any(|i| i.probe.is_some())
    }

    fn set(&mut self, i: usize, v: &ProbeVerdict, step: u32, at_done: bool) -> bool {
        let r = &mut self.items[i].rec;
        let before = (r.status, r.verified_at);
        if v.ok {
            if r.status != Status::Verified {
                r.status = Status::Verified;
                r.verified_at = Some(step);
                r.by = Some(VerifiedBy::Probe);
                r.evidence = std::mem::take(&mut self.confirmed);
            }
        } else if at_done && r.status == Status::Verified {
            r.status = Status::Reverted;
            r.by = None;
        }
        before != (r.status, r.verified_at)
    }

    /// Run the observation probes (no OS call) on outcomes not yet verified.
    /// Returns true when a status changed.
    pub fn observe(&mut self, obs: &Observation, step: u32) -> bool {
        let mut changed = false;
        for i in 0..self.items.len() {
            if self.items[i].rec.status == Status::Verified {
                continue;
            }
            let verdict = self.items[i].probe.as_ref().and_then(|p| p.on_screen(obs));
            if let Some(v) = verdict.filter(|v| v.ok) {
                changed |= self.set(i, &v, step, false);
            }
        }
        changed
    }

    /// The probed outcomes, for the gate: index, id and probe.
    pub fn probed(&self) -> Vec<(usize, String, ProbeSpec)> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| it.probe.clone().map(|p| (i, it.rec.id.clone(), p)))
            .collect()
    }

    /// Record a probe result taken when `done` was claimed.
    pub fn settle(&mut self, i: usize, v: &ProbeVerdict, step: u32) -> bool {
        self.set(i, v, step, true)
    }

    /// The user vouched that the task is done: mark who waived it.
    pub fn waive(&mut self) {
        for it in &mut self.items {
            if it.probe.is_some() && it.rec.status != Status::Verified {
                it.rec.by = Some(VerifiedBy::User);
            }
        }
    }

    /// Evidence from stage 2: confirmed actions are credited to the next
    /// outcome that becomes verified.
    pub fn note_effect(&mut self, step: u32, effect: Effect) {
        if effect == Effect::Confirmed && !self.confirmed.contains(&step) {
            self.confirmed.push(step);
        }
    }

    /// The records, with any one-time code Kai typed blanked out.
    pub fn records(&self, codes: &[OneTimeCode]) -> Vec<OutcomeRecord> {
        self.items
            .iter()
            .map(|it| {
                let mut r = it.rec.clone();
                r.description = redact_text(&r.description, codes);
                r.probe = r.probe.map(|p| redact_text(&p, codes));
                r.probe_refused = r.probe_refused.map(|p| redact_text(&p, codes));
                r
            })
            .collect()
    }
}

/// The JSON schema of the `outcomes` property (first call only).
pub fn outcomes_schema() -> Value {
    let s = serde_json::json!({ "type": ["string", "null"] });
    serde_json::json!({
        "type": "array",
        "maxItems": MAX_OUTCOMES,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["id", "description"],
            "properties": {
                "id": { "type": "string" },
                "description": { "type": "string" },
                "probe": {
                    "type": ["object", "null"],
                    "additionalProperties": false,
                    "required": ["kind"],
                    "properties": {
                        "kind": { "type": "string", "enum":
                            ["file_exists","file_contains","a11y_has","window_title","url_is","shell_ro"] },
                        "path": s, "text": s, "role": s, "name": s, "pattern": s, "cmd": s
                    }
                }
            }
        }
    })
}

/// The one-time instruction added to the first user turn when the ledger is
/// on. Kept out of the system prompt so the cached prefix does not change.
pub const OUTCOME_REQUEST: &str = "First step only: also add `outcomes`, 1-4 things \
that must be true when the task is done, each {\"id\":\"sent\",\"description\":\
\"the form was submitted\",\"probe\":{...}}. A probe is optional; Kai checks it \
before accepting done. Probes (`kind` + fields): file_exists (path), file_contains \
(path, text), a11y_has (role, name), window_title (pattern, a regex), url_is \
(pattern), shell_ro (cmd: a read-only ls, stat, grep, cat, test or wc). Pick probes \
that are false until the task is really done.";

/// Blank any one-time code Kai typed for the user out of `s` (the same rule
/// as the element-list redaction: spaces and dashes do not hide a code).
pub fn redact_text(s: &str, codes: &[OneTimeCode]) -> String {
    let mut out = s.to_string();
    for c in codes {
        let code = c.expose();
        if code.is_empty() {
            continue;
        }
        if out.contains(code) {
            out = out.replace(code, REDACTED_CODE);
        }
        let compact: String = out
            .chars()
            .filter(|ch| !ch.is_whitespace() && *ch != '-')
            .collect();
        if compact.contains(code) {
            return REDACTED_CODE.to_string();
        }
    }
    out
}

/// A gate record with every string redacted.
pub fn redact_gate(g: &mut GateRecord, codes: &[OneTimeCode]) {
    for f in &mut g.failed {
        f.outcome = redact_text(&f.outcome, codes);
        f.probe = redact_text(&f.probe, codes);
        f.reason = redact_text(&f.reason, codes);
    }
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;
