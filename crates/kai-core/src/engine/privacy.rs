//! A sensitive entry permanently disables pixel egress for this run. Field
//! masking alone cannot protect text an application copies elsewhere.

use crate::{Action, Decision, Grounding, Observation, Role};

#[derive(Default)]
pub(super) struct Privacy {
    secrets: Vec<String>,
    fields: Vec<(Role, String)>,
}

impl Privacy {
    pub fn active(&self) -> bool {
        !self.secrets.is_empty()
    }

    pub fn protect(&mut self, d: &Decision, g: &Grounding, force: bool) {
        if d.action != Action::Type
            || (!force
                && !crate::risk::is_payment_or_password(d, g)
                && crate::risk::sensitive_typed(d).is_none())
        {
            return;
        }
        if let Some(text) = d.text.as_ref().filter(|t| !t.is_empty()) {
            self.secrets.push(text.clone());
            if let Some(el) = d
                .target
                .or_else(|| g.focused_field())
                .and_then(|m| g.element(m))
            {
                self.fields.push((el.role, el.name.clone()));
            }
        }
    }

    pub fn decision(&self, d: &Decision) -> Decision {
        let mut d = d.clone();
        d.thought = self.text(&d.thought);
        d.expect = self.text(&d.expect);
        d.text = if let Some(kind) = crate::risk::sensitive_typed(&d) {
            Some(format!("[{kind}]"))
        } else {
            d.text.as_deref().map(|s| self.text(s))
        };
        d
    }
    pub fn record(&self, rec: &mut crate::trace::StepRecord) {
        rec.decision = self.decision(&rec.decision);
        rec.elements = self.text(&rec.elements);
        rec.screen = rec.screen.as_deref().map(|s| self.text(s));
        if let Some(target) = &mut rec.target {
            target.name = self.text(&target.name);
        }
        rec.output = rec.output.as_deref().map(|s| self.text(s));
        if let Some(executor) = &mut rec.executor {
            executor.subgoal = self.text(&executor.subgoal);
        }
        rec.call.prompt_user = rec.call.prompt_user.as_deref().map(|s| self.text(s));
        // Dropping raw replies prevents differently escaped JSON representations
        // from retaining a secret, including a batch from an earlier call.
        if self.active() {
            rec.call.reply_raw = None;
        }
    }

    fn value(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(s) => *s = self.text(s),
            serde_json::Value::Array(items) => items.iter_mut().for_each(|v| self.value(v)),
            serde_json::Value::Object(fields) => fields.values_mut().for_each(|v| self.value(v)),
            _ => {}
        }
    }
    pub fn trace(&self, trace: &mut crate::Trace) {
        if !self.active() {
            return;
        }
        // A serialization pass covers nested ledger, recovery and task data.
        if let Ok(mut value) = serde_json::to_value(&*trace) {
            self.value(&mut value);
            if let Ok(mut safe) = serde_json::from_value::<crate::Trace>(value) {
                safe.typed_secrets = std::mem::take(&mut trace.typed_secrets);
                *trace = safe;
                return;
            }
        }
        // A short password can equal an enum tag. Preserve typed decisions
        // and suppress optional nested metadata rather than panic or leak it.
        trace.task = self.text(&trace.task);
        trace.errors = trace.errors.iter().map(|s| self.text(s)).collect();
        trace.steered = trace.steered.iter().map(|s| self.text(s)).collect();
        trace.lessons.clear();
        trace.human.clear();
        trace.outcomes.clear();
        trace.gate.clear();
        trace.best_of = None;
        trace.recoveries.clear();
        for rec in &mut trace.steps {
            self.record(rec);
        }
    }

    pub fn text(&self, text: &str) -> String {
        self.secrets.iter().fold(text.to_string(), |s, secret| {
            crate::untrusted::blank(&s, secret, crate::untrusted::REDACTED_SENSITIVE)
        })
    }

    pub fn redact(&self, g: &mut Grounding) {
        if let Grounding::Marks(els) = g {
            for el in els {
                if self
                    .fields
                    .iter()
                    .any(|(role, name)| el.role == *role && el.name == *name)
                {
                    el.value = Some(crate::untrusted::REDACTED_SENSITIVE.into());
                } else if let Some(value) = &mut el.value {
                    *value = self.text(value);
                }
                el.name = self.text(&el.name);
            }
        }
    }

    pub fn sanitise(&self, obs: &mut Observation) {
        if self.active() {
            obs.images.screenshot_jpeg.clear();
            obs.images.crop_jpeg = None;
            self.redact(&mut obs.grounding);
            obs.window_title = obs.window_title.as_deref().map(|s| self.text(s));
            obs.url = obs.url.as_deref().map(|s| self.text(s));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secure_focused_field_latches_and_redacts_partial_values() {
        let d = Decision {
            action: Action::Type,
            thought: String::new(),
            target: None,
            text: Some("ordinary-password".into()),
            keys: vec![],
            direction: None,
            amount: None,
            risk: Default::default(),
            expect: String::new(),
        };
        let mut g = Grounding::Marks(vec![crate::Element {
            mark: 1,
            role: Role::PasswordField,
            name: "Password".into(),
            bounds: crate::Rect::new(0., 0., 10., 10.),
            enabled: true,
            secure: true,
            focused: true,
            value: Some("ordi".into()),
        }]);
        let mut p = Privacy::default();
        p.protect(&d, &g, false);
        assert!(p.active());
        p.redact(&mut g);
        assert_eq!(
            g.element(1).unwrap().value.as_deref(),
            Some(crate::untrusted::REDACTED_SENSITIVE)
        );
    }
}
