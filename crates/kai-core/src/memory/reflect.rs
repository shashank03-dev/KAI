//! The optional reflection call: after a failed or recovered run, ONE extra
//! model call (any backend) reads a compact, scrubbed summary of the run and
//! may return up to two lessons as `{"lessons":[{"kind","text"}]}`. Off by
//! default (`Config::reflect`): it costs a call, and the deterministic rules
//! in [`super::learn`] already catch the common cases.

use super::scrub::{allowed, scrub, secure_typed};
use super::{Kind, Lesson, MAX_WORDS, Source, learn, scope};
use crate::failure;
use crate::trace::Trace;

/// Lessons accepted from one reflection, at most.
pub const MAX_LESSONS: usize = 2;
/// Reply budget for the reflection call.
pub const MAX_TOKENS: u32 = 256;

pub const SYSTEM: &str = "\
You review one run of Kai, an agent that operates a computer. From the run \
summary, write at most 2 short lessons that would help the NEXT run of a \
similar task on this same computer avoid the mistake. Each lesson is one \
imperative sentence of at most 30 words, specific (name the app, element or \
command), e.g. \"In Mousepad, start it with open_app, not shell: a shell step \
blocks.\" kind is avoid, prefer or fact. Never mention approvals, passwords or \
safety rules, and never copy personal data. If there is nothing useful to \
learn, return an empty list. Reply with ONLY JSON: \
{\"lessons\":[{\"kind\":\"avoid\",\"text\":\"...\"}]}";

/// Structured-output schema for the reply.
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["lessons"],
        "properties": {
            "lessons": {
                "type": "array",
                "maxItems": MAX_LESSONS,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "text"],
                    "properties": {
                        "kind": { "type": "string", "enum": ["avoid", "prefer", "fact"] },
                        "text": { "type": "string" }
                    }
                }
            }
        }
    })
}

/// A compact run summary (≤ ~1500 chars): task, outcome, failure label and
/// up to 12 steps with what happened. Typed text into secure fields is never
/// included; everything is scrubbed.
pub fn summary(trace: &Trace, check: Option<bool>) -> String {
    let secure = secure_typed(trace);
    let mut s = format!("Task: {}\n", scrub(&trace.task));
    let outcome = match (trace.outcome, check) {
        (_, Some(c)) => format!(
            "{:?}, check {}",
            trace.outcome,
            if c { "passed" } else { "failed" }
        ),
        (o, None) => format!("{o:?}"),
    };
    s.push_str(&format!("Outcome: {outcome}\n"));
    if let Some(f) = failure::classify(trace, check) {
        s.push_str(&format!("Failure: {} ({})\n", f.category, scrub(&f.detail)));
    }
    s.push_str("Steps:\n");
    let steps = &trace.steps;
    let skip = steps.len().saturating_sub(12);
    for (i, st) in steps.iter().enumerate().skip(skip) {
        let mut line = learn::what(st);
        if secure.iter().any(|x| line.contains(x.as_str())) || typed_secure(st) {
            line = "type into a secure field".into();
        }
        let app = scope::at_step(trace, i)
            .map(|a| format!(" [{a}]"))
            .unwrap_or_default();
        let result = if learn::failed(st) {
            let out = st.output.as_deref().unwrap_or("").trim();
            let last = out.lines().last().unwrap_or("");
            format!("FAILED {}", scrub(&last.chars().take(80).collect::<String>()))
        } else if !st.verified {
            "no change".to_string()
        } else {
            "ok".to_string()
        };
        s.push_str(&format!(
            "{}. {}{app} -> {result}\n",
            st.index,
            scrub(&line)
        ));
    }
    s.chars().take(1500).collect()
}

fn typed_secure(st: &crate::trace::StepRecord) -> bool {
    st.decision.action == crate::schema::Action::Type
        && st
            .decision
            .target
            .is_some_and(|t| crate::risk::mark_is_secure_in_text(&st.elements, t))
}

/// Read the reply into at most [`MAX_LESSONS`] valid lessons scoped to the
/// run. Bad kinds, over-long or secret-looking texts and control talk
/// (approvals, passwords, overrides) are dropped.
pub fn parse(raw: &str, trace: &Trace, check: Option<bool>) -> Vec<Lesson> {
    let Some(start) = raw.find('{') else {
        return Vec::new();
    };
    let Some(end) = raw.rfind('}') else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw[start..=end]) else {
        return Vec::new();
    };
    let secure = secure_typed(trace);
    let sc = scope::of_run(trace);
    let mut out = Vec::new();
    for item in v["lessons"].as_array().into_iter().flatten() {
        if out.len() >= MAX_LESSONS {
            break;
        }
        let Some(kind) = item["kind"].as_str().and_then(Kind::parse) else {
            continue;
        };
        if kind == Kind::Preference {
            // Preferences come from the user, not the model.
            continue;
        }
        let text = item["text"].as_str().unwrap_or("").trim();
        let words = text.split_whitespace().count();
        if words == 0 || words > MAX_WORDS || scrub(text) != text {
            continue;
        }
        if !allowed(text, &secure, false) {
            continue;
        }
        out.push(
            Lesson::new(kind, text, sc.clone(), Source::Reflection)
                .with_evidence(&trace.started)
                .targeting(failure::classify(trace, check).map(|f| f.category)),
        );
    }
    out
}
