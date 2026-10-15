//! A sensitive entry permanently disables pixel egress for this run. Field
//! masking alone cannot protect text an application copies elsewhere.

use crate::{Action, Decision, Grounding, Observation, Role};

#[derive(Default)]
pub(super) struct Privacy {
    secrets: Vec<String>,
    fields: Vec<(Role, String)>,
}

impl Privacy {
    pub fn active(&self) -> bool { !self.secrets.is_empty() }

    pub fn protect(&mut self, d: &Decision, g: &Grounding, force: bool) {
        if d.action != Action::Type || (!force && crate::risk::sensitive_typed(d).is_none()) {
            return;
        }
        if let Some(text) = d.text.as_ref().filter(|t| !t.is_empty()) {
            self.secrets.push(text.clone());
            if let Some(el) = d.target.and_then(|m| g.element(m)) {
                self.fields.push((el.role, el.name.clone()));
            }
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
                if self.fields.iter().any(|(role, name)| el.role == *role && el.name == *name) {
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
