//! One model turn on OpenAI's computer contract (`crate::oai_contract`):
//! the open model gets OpenAI-style instructions and, for pixel-trained
//! models, the unmarked frame; its action comes back through the OpenAI
//! parser and then takes Kai's normal computer-tool path.
use super::Engine;
use super::record::{Decided, DecisionObservation};
use crate::client::{Images, ModelClient, ModelRequest};
use crate::oai_contract::{self, CoordFrame, Turn};
use crate::reply::StepReply;
use serde_json::json;

impl Engine<'_> {
    pub(super) async fn decide_oai(
        &self,
        client: &dyn ModelClient,
        frame: CoordFrame,
        user_text: &str,
        observed: DecisionObservation<'_>,
        max_tokens: u32,
    ) -> anyhow::Result<Decided> {
        let obs = observed.obs;
        let seen = crate::cua::jpeg_size(&obs.images.screenshot_jpeg);
        // Only when this step may show the screen at all: an empty image is
        // a tree-only step or the privacy latch, never to be bypassed.
        let native = match (frame.unmarked(), seen) {
            (true, Some(_)) => self.perceptor.native_frame().await,
            _ => None,
        };
        let (images, size) = match &native {
            Some(f) => (
                Images {
                    screenshot_jpeg: f.image.clone(),
                    crop_jpeg: None,
                },
                (f.width, f.height),
            ),
            None => (obs.images.clone(), seen.unwrap_or((0, 0))),
        };
        anyhow::ensure!(
            !frame.unmarked() || size.0 > 0,
            "a pixel-frame model needs a screenshot on this step"
        );
        let system = oai_contract::system(frame, size);
        let schema = oai_contract::schema(frame);
        let focused = obs.grounding.focused_field();
        let req = ModelRequest {
            observation_id: Some(observed.id),
            computer_result: observed.result,
            system: &system,
            user_text,
            images: &images,
            schema: &schema,
            max_tokens,
            temperature: 0.0,
        };
        let decode = |raw: &str| oai_contract::decode(raw, frame, size, observed.id, focused);
        let reply = self.decision_request(client, &req).await?;
        let (turn, reply) = match decode(&reply.raw) {
            Ok(turn) => (turn, reply),
            Err(e) => {
                log::warn!("repairing a contract reply: {e}");
                let repair = format!(
                    "{user_text}\n\nYour previous reply was not valid ({e}). Reply with ONLY the \
                     JSON object: thought, actions (at most one), message, phase."
                );
                let retry = ModelRequest {
                    user_text: &repair,
                    ..req
                };
                let second = self.decision_request(client, &retry).await?;
                (decode(&second.raw)?, second)
            }
        };
        let raw = match turn {
            Turn::Act {
                thought,
                mut action,
            } => {
                // Coordinates of the larger unmarked frame, onto the frame
                // Kai observed and maps to the screen.
                if let Some(seen) = seen {
                    oai_contract::rescale(&mut action, size, seen);
                }
                json!({"thought":thought,"action":"computer","computer":action,"risk":"low"})
            }
            Turn::Done { message } => {
                json!({"thought":message,"action":"done","risk":"none"})
            }
            Turn::Ask { message } => {
                json!({"thought":"needs the user","action":"ask_user","text":message,"risk":"none"})
            }
        };
        let step = StepReply::parse_computer(&raw.to_string(), focused)?;
        Ok(Decided::new(step, &reply))
    }
}
