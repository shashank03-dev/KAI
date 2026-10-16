//! One model turn on OpenAI's computer contract (`crate::oai_contract`):
//! the open model gets OpenAI-style instructions and, for pixel-trained
//! models, the unmarked frame; its action comes back through the OpenAI
//! parser and then takes Kai's normal computer-tool path.
use super::record::{Decided, DecisionObservation};
use super::{Engine, Run};
use crate::client::{Images, ModelClient, ModelReply, ModelRequest};
use crate::computer::{ComputerAction, Target};
use crate::oai_contract::{self, CoordFrame, LATER_FOCUS, Turn};
use crate::ports::Observation;
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
        let batch = self.config.oai_batch.max(1) as usize;
        // No marks to name targets by: a grounder finds described ones.
        let describe =
            self.grounder.is_some() && matches!(obs.grounding, crate::Grounding::Grid { .. });
        let system = oai_contract::system(frame, size, batch, describe);
        let schema = oai_contract::schema(frame, batch, describe);
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
        let decode =
            |raw: &str| oai_contract::decode(raw, frame, size, observed.id, focused, batch);
        let k = self.config.step_candidates.max(1) as usize;
        if k > 1 {
            let sampled = ModelRequest {
                temperature: 0.7,
                ..req
            };
            let replies =
                futures::future::join_all((0..k).map(|_| self.decision_request(client, &sampled)))
                    .await;
            let replies: Vec<ModelReply> = replies.into_iter().filter_map(Result::ok).collect();
            let mut good: Vec<(Turn, ModelReply)> = Vec::new();
            let mut rejected = Vec::new();
            for r in replies {
                let raw = self.ground(&r.raw, frame, &images, size, describe).await;
                if let Some(t) = raw.ok().and_then(|raw| decode(&raw).ok()) {
                    good.push((t, r));
                } else {
                    rejected.push(r);
                }
            }
            let i = oai_contract::vote(good.iter().map(|(t, _)| t))
                .ok_or_else(|| anyhow::anyhow!("none of {k} candidate replies decoded"))?;
            let (turn, reply) = good.swap_remove(i);
            let mut decided = self.turn_decided(turn, &reply, size, seen, focused)?;
            for (_, other) in &good {
                decided.add_usage(other);
            }
            for other in &rejected {
                decided.add_usage(other);
            }
            return Ok(decided);
        }
        let reply = self.decision_request(client, &req).await?;
        let grounded = self
            .ground(&reply.raw, frame, &images, size, describe)
            .await?;
        let (turn, reply, earlier) = match decode(&grounded) {
            Ok(turn) => (turn, reply, None),
            Err(e) => {
                log::warn!("repairing a contract reply: {e}");
                let repair = format!(
                    "{user_text}\n\nYour previous reply was not valid ({e}). Reply with ONLY the \
                     JSON object: thought, actions (at most {batch}), message, phase."
                );
                let retry = ModelRequest {
                    user_text: &repair,
                    ..req
                };
                let second = self.decision_request(client, &retry).await?;
                let grounded = self
                    .ground(&second.raw, frame, &images, size, describe)
                    .await?;
                (decode(&grounded)?, second, Some(reply))
            }
        };
        let mut decided = self.turn_decided(turn, &reply, size, seen, focused)?;
        if let Some(earlier) = earlier.as_ref() {
            decided.add_usage(earlier);
        }
        Ok(decided)
    }

    /// A decoded turn as the step's decision; later actions are queued.
    fn turn_decided(
        &self,
        turn: Turn,
        reply: &ModelReply,
        size: (u32, u32),
        seen: Option<(u32, u32)>,
        focused: Option<u32>,
    ) -> anyhow::Result<Decided> {
        let mut queued = Vec::new();
        let raw = match turn {
            Turn::Act { thought, actions } => {
                let mut actions = actions.into_iter();
                let mut first = actions.next().ok_or_else(|| anyhow::anyhow!("no action"))?;
                queued.extend(actions);
                // Coordinates of the larger unmarked frame, onto the frame
                // Kai observed and maps to the screen.
                if let Some(seen) = seen {
                    for a in std::iter::once(&mut first).chain(queued.iter_mut()) {
                        oai_contract::rescale(a, size, seen);
                    }
                }
                json!({"thought":thought,"action":"computer","computer":first,"risk":"low"})
            }
            Turn::Done { message } => {
                json!({"thought":message,"action":"done","risk":"none"})
            }
            Turn::Ask { message } => {
                json!({"thought":"needs the user","action":"ask_user","text":message,"risk":"none"})
            }
        };
        let step = StepReply::parse_computer(&raw.to_string(), focused)?;
        let mut decided = Decided::new(step, reply);
        decided.queued = queued;
        Ok(decided)
    }
}

impl Engine<'_> {
    /// The next queued action of a multi-action turn, made current for
    /// observation `id`; `None` when the queue is empty or must stop (the last
    /// action failed, the layout changed, no field has focus to type in), in
    /// which case the model's next prompt says what ran.
    pub(super) fn next_queued(
        &self,
        r: &mut Run<'_>,
        obs: &Observation,
        id: u64,
    ) -> Option<ComputerAction> {
        let mut action = r.queued.pop_front()?;
        let failed = r.computer_result.as_ref().is_none_or(|x| x.error.is_some());
        let moved = !r
            .queued_on
            .as_ref()
            .is_some_and(|g| super::fresh::same_layout(g, &obs.grounding));
        let focus = obs.grounding.focused_field();
        let why = if failed {
            Some("the last one was skipped or failed")
        } else if moved {
            Some("the screen layout changed")
        } else if matches!(action, ComputerAction::Type { .. }) && focus.is_none() {
            Some("no field has keyboard focus to type in")
        } else {
            None
        };
        let (total, handed) = r.queued_turn;
        if let Some(why) = why {
            r.queued.clear();
            let ran = handed - usize::from(failed);
            r.notes.push(format!(
                "Harness note: {}",
                oai_contract::stopped_note(ran, total, why)
            ));
            return None;
        }
        let refresh = |t: &mut Target| match t {
            Target::Point { observation, .. } | Target::Element { observation, .. } => {
                *observation = id
            }
        };
        match &mut action {
            ComputerAction::Type { target, .. } => {
                *target = Target::Element {
                    observation: id,
                    mark: focus.unwrap_or(LATER_FOCUS),
                };
            }
            a => a.targets_mut().into_iter().for_each(refresh),
        }
        r.queued_turn.1 += 1;
        Some(action)
    }

    /// A queued action as this step's decision: no model call.
    pub(super) fn queued_decision(
        &self,
        client: &dyn ModelClient,
        action: ComputerAction,
    ) -> anyhow::Result<Decided> {
        let raw = json!({"thought":"next action of the same turn","action":"computer","computer":action,"risk":"low"})
            .to_string();
        let reply = ModelReply {
            raw: raw.clone(),
            prompt_tokens: None,
            completion_tokens: None,
            first_token: None,
            total: std::time::Duration::ZERO,
            backend: client.backend(),
            cost_usd: None,
        };
        Ok(Decided::new(StepReply::parse_computer(&raw, None)?, &reply))
    }

    /// Keep the rest of a multi-action turn for the next steps.
    pub(super) fn queue_rest(r: &mut Run<'_>, obs: &Observation, rest: Vec<ComputerAction>) {
        if rest.is_empty() {
            return;
        }
        r.queued_turn = (rest.len() + 1, 1);
        r.queued = rest.into();
        r.queued_on = Some(obs.grounding.clone());
    }

    /// Fill in x and y for actions that describe their target instead
    /// (`target`), using the grounding model on the same image.
    async fn ground(
        &self,
        raw: &str,
        frame: CoordFrame,
        images: &Images,
        size: (u32, u32),
        describe: bool,
    ) -> anyhow::Result<String> {
        let Some(g) = self.grounder else {
            return Ok(raw.to_string());
        };
        let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}')) else {
            return Ok(raw.to_string());
        };
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw[start..=end]) else {
            return Ok(raw.to_string());
        };
        let Some(actions) = v["actions"].as_array_mut() else {
            return Ok(raw.to_string());
        };
        for a in actions {
            let Some(what) = a["target"].as_str().map(str::to_string) else {
                continue;
            };
            anyhow::ensure!(
                describe,
                "a described target is only valid on an unmarked screen"
            );
            if !a["x"].is_null() || !a["y"].is_null() || !a["mark"].is_null() {
                continue;
            }
            let (x, y) = self.locate(g, &what, images, size).await?;
            let (fx, fy) = frame.from_image(x, y, size);
            a["x"] = json!(fx.floor() as i64);
            a["y"] = json!(fy.floor() as i64);
        }
        Ok(v.to_string())
    }

    /// Where `what` is, in pixels of the `size` image.
    async fn locate(
        &self,
        g: super::Grounder<'_>,
        what: &str,
        images: &Images,
        size: (u32, u32),
    ) -> anyhow::Result<(f64, f64)> {
        let system = oai_contract::grounding(g.frame, size);
        let schema = json!({"type":"object","additionalProperties":false,"required":["x","y"],
            "properties":{"x":{"type":"integer"},"y":{"type":"integer"}}});
        let req = ModelRequest {
            observation_id: None,
            computer_result: None,
            system: &system,
            user_text: what,
            images,
            schema: &schema,
            max_tokens: 64,
            temperature: 0.0,
        };
        let reply = self.decision_request(g.client, &req).await?;
        let raw = &reply.raw;
        let v: serde_json::Value = raw
            .find('{')
            .zip(raw.rfind('}'))
            .and_then(|(a, b)| serde_json::from_str(&raw[a..=b]).ok())
            .ok_or_else(|| anyhow::anyhow!("the grounder's reply was not a point"))?;
        let point = v["x"].as_f64().zip(v["y"].as_f64());
        point
            .and_then(|(x, y)| g.frame.to_image(x, y, size))
            .ok_or_else(|| anyhow::anyhow!("the grounder put \"{what}\" off the screen"))
    }
}
