//! Native model transport: use the original model's text dialect, then hand
//! canonical actions back to Kai's existing supervision and executor.
use super::Engine;
use super::record::{Decided, DecisionObservation};
use crate::client::{Images, ModelClient, ModelReply, ModelRequest};
use crate::native_models::{NativeModel, NativeTurn};
use crate::reply::StepReply;
use serde_json::{Value, json};

impl Engine<'_> {
    pub(super) async fn decide_native(
        &self,
        client: &dyn ModelClient,
        model: NativeModel,
        user_text: &str,
        observed: DecisionObservation<'_>,
        max_tokens: u32,
    ) -> anyhow::Result<Decided> {
        let seen = crate::cua::jpeg_size(&observed.obs.images.screenshot_jpeg);
        anyhow::ensure!(
            seen.is_some(),
            "native computer-use requires a visible screenshot"
        );
        let native = self.perceptor.native_frame().await;
        // Privacy latch: native_frame can never restore a hidden image.
        let (images, size) = match native {
            Some(ref frame) => (
                Images {
                    screenshot_jpeg: frame.image.clone(),
                    crop_jpeg: None,
                },
                (frame.width, frame.height),
            ),
            None => (observed.obs.images.clone(), seen.unwrap()),
        };
        anyhow::ensure!(
            size.0 > 0 && size.1 > 0,
            "native screenshot has invalid dimensions"
        );
        // Fara and UI-TARS use their original text dialects; Holo uses actual
        // API function calls. None uses Kai's standard decision JSON schema.
        let schema = Value::Null;
        let system = model.system(size);
        let tool = model.tool_spec();
        let req = ModelRequest {
            observation_id: Some(observed.id),
            computer_result: observed.result,
            system: &system,
            user_text,
            images: &images,
            schema: &schema,
            max_tokens: max_tokens.max(512),
            temperature: 0.,
        };
        let focused = observed.obs.grounding.focused_field();
        let decode = |raw: &str| -> anyhow::Result<StepReply> {
            let turn = model.decode(raw, observed.id, focused, size)?;
            let value = match turn {
                NativeTurn::Act(mut action) => {
                    // The screenshot the model saw may be larger than the
                    // sanitized observation frame used by the executor.
                    crate::oai_contract::rescale(&mut action, size, seen.unwrap());
                    json!({"thought":"native model action", "action":"computer", "computer":action, "risk":"high"})
                }
                NativeTurn::Done(message) => {
                    json!({"thought":message, "action":"done", "risk":"none"})
                }
                NativeTurn::Ask(question) => {
                    json!({"thought":"needs user", "action":"ask_user", "text":question, "risk":"none"})
                }
            };
            Ok(StepReply::parse_computer(&value.to_string(), focused)?)
        };
        let first = self.native_request(client, &req, tool.as_ref()).await?;
        match decode(&first.raw) {
            Ok(step) => Ok(Decided::new(step, &first)
                .record_request(&req, observed.recording)
                .record_tool_spec(tool.as_ref())),
            Err(error) => {
                log::warn!("invalid native model reply: {error}");
                let retry_text = format!(
                    "{user_text}\n\nYour previous native tool call was invalid: {error}. \
                     Return one valid action using the original native format."
                );
                let retry = ModelRequest {
                    user_text: &retry_text,
                    max_tokens: max_tokens.max(768),
                    ..req
                };
                let second = self.native_request(client, &retry, tool.as_ref()).await?;
                let mut decided = Decided::new(decode(&second.raw)?, &second);
                decided.add_usage(&first);
                Ok(decided
                    .record_request(&retry, observed.recording)
                    .record_tool_spec(tool.as_ref()))
            }
        }
    }

    async fn native_request(
        &self,
        client: &dyn ModelClient,
        req: &ModelRequest<'_>,
        tool: Option<&Value>,
    ) -> anyhow::Result<ModelReply> {
        let Some(tool) = tool else {
            return self.decision_request(client, req).await;
        };
        anyhow::ensure!(!self.interrupt.cancelled(), "run cancelled");
        tokio::select! {
            biased;
            () = crate::ports::wait_cancelled(self.interrupt) => anyhow::bail!("run cancelled"),
            reply = client.decide_with_tool(req, tool) => Ok(reply?),
        }
    }
}
