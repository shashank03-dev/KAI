//! The outcome ledger in the loop, and the DONE gate (`docs/VERIFICATION.md`):
//! - the first model call also asks for 1–4 required outcomes (no extra
//!   call; the request rides in that turn, not in the cached system prompt);
//! - every observation runs the observation probes (`a11y_has`,
//!   `window_title`, `url_is`), which cost no OS call;
//! - `done` is accepted only when every outcome with a probe holds now; OS
//!   probes run then, each under `Config::probe_timeout`;
//! - a refused `done` tells the model why and forces a full frame; after
//!   [`MAX_GATE_REJECTIONS`] Kai asks the user instead of looping.
//!
//! Only probe results change an outcome's status (and "I did it" waives the
//! gate); the model's replies, its thought and memory never do.

use super::{Engine, Run};
use crate::human::HumanCheck;
use crate::ledger::{
    Effect, GateFail, GateRecord, Ledger, OUTCOME_REQUEST, redact_gate, redact_text,
};
use crate::ports::{EngineEvent, Observation, Probe};
use crate::probe::{ProbeSpec, ProbeVerdict, clip};
use crate::prompt::HistoryItem;
use crate::reply::StepReply;
use crate::trace::{Outcome, StepRecord};

/// Refused `done`s (since the last time the user was asked) before Kai asks
/// the user what to do.
pub const MAX_GATE_REJECTIONS: u32 = 3;
/// Extra reply budget on the first call, for the outcomes.
pub const FIRST_CALL_TOKENS: u32 = 256;

/// The ledger and gate state of one run.
#[derive(Default)]
pub(super) struct LedgerState {
    ledger: Ledger,
    /// The first call's reply was read (outcomes are named only then).
    asked: bool,
    rejections: u32,
    /// The user said the task is done although a probe failed.
    waived: bool,
    /// The effect of the last labelled action (stage 2).
    pub(super) last_effect: Option<Effect>,
}

impl Engine<'_> {
    /// On the run's first model call with the ledger on: add the request
    /// to the turn's notes and return the schema that allows `outcomes`,
    /// plus the extra reply tokens. `None` otherwise (nothing changes).
    pub(super) fn outcome_request(
        &self,
        r: &Run<'_>,
        notes: &mut Vec<String>,
        schema: &serde_json::Value,
    ) -> Option<(serde_json::Value, u32)> {
        if !self.config.outcomes || r.ledger.asked {
            return None;
        }
        notes.push(OUTCOME_REQUEST.to_string());
        Some((StepReply::with_outcomes(schema), FIRST_CALL_TOKENS))
    }

    /// Read the outcomes from the first decoded reply. Later replies'
    /// `outcomes` are never read: the ledger is fixed from here on.
    pub(super) fn ledger_start(&self, r: &mut Run<'_>, step: u32, raw: &[serde_json::Value]) {
        if !self.config.outcomes || r.ledger.asked {
            return;
        }
        r.ledger.asked = true;
        r.ledger.ledger = Ledger::from_raw(raw);
        if !r.ledger.ledger.is_empty() {
            log::info!(
                "outcome ledger: {} outcome(s), gated: {}",
                r.ledger.ledger.records(&[]).len(),
                r.ledger.ledger.gated()
            );
            self.emit_outcomes(r, step);
        }
    }

    /// Observation probes on every observation; no OS call.
    pub(super) fn ledger_observe(&self, r: &mut Run<'_>, step: u32, obs: &Observation) {
        if r.ledger.ledger.observe(obs, step) {
            self.emit_outcomes(r, step);
        }
    }

    /// Stage 2 evidence: the label of the action(s) just checked.
    pub(super) fn ledger_effect(&self, r: &mut Run<'_>, step: u32, effect: Effect) {
        r.ledger.last_effect = Some(effect);
        r.ledger.ledger.note_effect(step, effect);
    }

    fn emit_outcomes(&self, r: &Run<'_>, step: u32) {
        self.sink.emit(&EngineEvent::Outcomes {
            step,
            outcomes: r.ledger.ledger.records(&r.human.codes),
        });
    }

    /// The model said `done`. Accept it (as before) unless an outcome with a
    /// probe fails now; then refuse the step and tell the model. `Some` ends
    /// the run.
    pub(super) async fn claim_done(
        &self,
        r: &mut Run<'_>,
        mut rec: StepRecord,
        obs: &Observation,
    ) -> Option<Outcome> {
        let step = rec.index;
        if !r.ledger.ledger.gated() || r.ledger.waived {
            rec.path = Some("none".into());
            r.trace.push(rec);
            return Some(Outcome::Success);
        }
        let failed = self.run_probes(r, step, obs).await;
        self.emit_outcomes(r, step);
        if failed.is_empty() {
            rec.path = Some("none".into());
            r.trace.push(rec);
            return Some(Outcome::Success);
        }

        // ---- Refuse: not done yet ----
        let codes = &r.human.codes;
        let why = failed
            .iter()
            .map(|f| format!("{}: {} failed ({})", f.outcome, f.probe, f.reason))
            .collect::<Vec<_>>()
            .join("; ");
        let msg = redact_text(&format!("not done: {why}"), codes);
        rec.path = Some("gate".into());
        rec.verified = false;
        r.trace.push(rec);
        let mut g = GateRecord {
            step,
            kind: "refused".into(),
            failed,
            last_effect: r.ledger.last_effect,
        };
        redact_gate(&mut g, codes);
        r.trace.gate.push(g);
        let mut note = format!(
            "{msg}. Kai checked: the task is not finished yet. Keep going, then say done again."
        );
        if r.ledger.last_effect == Some(Effect::NoOp) {
            note.push_str(" Your last action changed nothing.");
        }
        r.notes.push(note);
        r.history.push(HistoryItem::note(format!(
            "done refused by Kai's check: {msg}"
        )));
        r.pending = None;
        r.force_full = true;
        self.sink.emit(&EngineEvent::Gated {
            step,
            reason: msg.clone(),
        });
        log::info!("DONE gate refused step {step}: {msg}");

        r.ledger.rejections += 1;
        if r.ledger.rejections < MAX_GATE_REJECTIONS {
            return None;
        }
        // ---- Bounded: ask the user instead of looping ----
        r.ledger.rejections = 0;
        let q = HumanCheck::question(&format!(
            "Kai thinks the task is done but could not confirm it ({msg}). Press \"I did it\" \
             (or Enter) if it is done, or tell Kai what is missing."
        ));
        let before = r.trace.human.len();
        if let Some(o) = self.pause_for_human(r, q, None, "").await {
            return Some(o);
        }
        if r.trace
            .human
            .get(before)
            .is_some_and(|h| h.result == "resolved")
        {
            r.ledger.waived = true;
            r.ledger.ledger.waive();
            r.trace.gate.push(GateRecord {
                step: r.step.saturating_sub(1),
                kind: "waived_by_user".into(),
                failed: Vec::new(),
                last_effect: None,
            });
            self.emit_outcomes(r, step);
        }
        None
    }

    /// Check every probed outcome now and settle its status; return the
    /// failures.
    async fn run_probes(&self, r: &mut Run<'_>, step: u32, obs: &Observation) -> Vec<GateFail> {
        let mut failed = Vec::new();
        for (i, id, probe) in r.ledger.ledger.probed() {
            let v = match probe.on_screen(obs) {
                Some(v) => v,
                None => self.os_probe(r.probe, &probe).await,
            };
            if !v.ok {
                failed.push(GateFail {
                    outcome: id,
                    probe: probe.label(),
                    reason: v.reason.clone(),
                });
            }
            r.ledger.ledger.settle(i, &v, step);
        }
        failed
    }

    /// One OS probe under the timeout. No port, an error or a timeout fails
    /// the probe (fail closed).
    async fn os_probe(&self, port: Option<&dyn Probe>, probe: &ProbeSpec) -> ProbeVerdict {
        let Some(port) = port else {
            return ProbeVerdict::fail("no probe available");
        };
        let limit = self.config.probe_timeout;
        match tokio::time::timeout(limit, port.check(probe)).await {
            Err(_) => ProbeVerdict::fail(format!("timed out after {} ms", limit.as_millis())),
            Ok(Err(e)) => ProbeVerdict::fail(clip(&e.to_string(), 80)),
            Ok(Ok(v)) => ProbeVerdict {
                ok: v.ok,
                reason: clip(&v.reason, 80),
            },
        }
    }

    /// Descriptions of the outcomes verified so far, redacted, for the
    /// macro-history summary.
    pub(super) fn verified_outcomes(&self, r: &Run<'_>) -> Vec<String> {
        r.ledger
            .ledger
            .records(&r.human.codes)
            .into_iter()
            .filter(|o| o.status == crate::ledger::Status::Verified)
            .map(|o| o.description)
            .collect()
    }

    /// At the end of the run: the outcome records and every gate record,
    /// with any one-time code Kai typed blanked out.
    pub(super) fn finish_ledger(&self, r: &mut Run<'_>) {
        r.trace.outcomes = r.ledger.ledger.records(&r.human.codes);
        for g in &mut r.trace.gate {
            redact_gate(g, &r.human.codes);
        }
    }
}
