//! Network adaptation only. All execution belongs to the shared Engine.
use super::{CuaConfig, jpeg_size};
use crate::client::{
    Backend, ClientError, ModelClient, ModelReply, ModelRequest, http_client, send_error,
};
use crate::{Perceptor, computer::*, cua_protocol};
use async_trait::async_trait;
use base64::Engine as _;
use serde_json::{Value, json};
use std::{collections::VecDeque, time::Instant};
use tokio::sync::Mutex;
struct State {
    previous: Option<String>,
    calls: Vec<String>,
    actions: VecDeque<ComputerAction>,
    message: Option<String>,
    error: Option<String>,
    image: Option<(u32, u32)>,
    origin_image: Vec<u8>,
    origin_tree: Option<crate::Grounding>,
}
pub(super) struct Provider<'a> {
    caps: Capabilities,
    config: &'a CuaConfig,
    task: &'a str,
    eyes: &'a dyn Perceptor,
    key: String,
    http: reqwest::Client,
    state: Mutex<State>,
}
impl<'a> Provider<'a> {
    pub fn new(
        config: &'a CuaConfig,
        task: &'a str,
        eyes: &'a dyn Perceptor,
        caps: Capabilities,
    ) -> Result<Self, ClientError> {
        let key = config
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
            .ok_or(ClientError::MissingKey("OPENAI_API_KEY"))?;
        Ok(Self {
            caps,
            config,
            task,
            eyes,
            key,
            http: http_client(),
            state: Mutex::new(State {
                previous: None,
                calls: vec![],
                actions: VecDeque::new(),
                message: None,
                error: None,
                image: None,
                origin_image: Vec::new(),
                origin_tree: None,
            }),
        })
    }
    pub async fn finish(&self) -> (Option<String>, Option<String>) {
        let s = self.state.lock().await;
        (s.message.clone(), s.error.clone())
    }
    async fn post(&self, body: &Value) -> Result<Value, ClientError> {
        let response = self
            .http
            .post(format!(
                "{}/responses",
                self.config.base.trim_end_matches('/')
            ))
            .bearer_auth(&self.key)
            .json(body)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| send_error(e, self.config.timeout))?;
        let status = response.status();
        let text = response.text().await.map_err(|_| ClientError::EmptyBody)?;
        if !status.is_success() {
            return Err(ClientError::from_status(status.as_u16(), &text));
        }
        serde_json::from_str(&text).map_err(|_| ClientError::EmptyBody)
    }
}
fn reply(raw: Value, started: Instant) -> ModelReply {
    ModelReply {
        backend: Backend::OpenAiNative,
        cost_usd: None,
        raw: raw.to_string(),
        total: started.elapsed(),
        first_token: None,
        prompt_tokens: None,
        completion_tokens: None,
    }
}
fn ask(why: &str, started: Instant) -> ModelReply {
    reply(
        json!({"thought":"needs user verification","action":"ask_user","text":why,"risk":"high"}),
        started,
    )
}
#[async_trait]
impl ModelClient for Provider<'_> {
    fn backend(&self) -> Backend {
        Backend::OpenAiNative
    }
    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        let started = Instant::now();
        let mut state = self.state.lock().await;
        let Some(observation) = req.observation_id else {
            return Ok(ask("Missing host observation identity", started));
        };
        let Some(image) = jpeg_size(&req.images.screenshot_jpeg) else {
            return Ok(ask(
                "Computer use needs a safe screenshot; please continue manually",
                started,
            ));
        };
        if state.image.is_some_and(|previous| previous != image) && !state.actions.is_empty() {
            return Ok(ask(
                "Display dimensions changed during the provider batch",
                started,
            ));
        }
        if !state.actions.is_empty() {
            let fresh = self
                .eyes
                .observe_tree_only()
                .await
                .map_err(|_| ClientError::EmptyBody)?;
            if req.computer_result.is_none_or(|r| r.error.is_some())
                || req.images.screenshot_jpeg != state.origin_image
                || state.origin_tree.as_ref() != Some(&fresh.grounding)
            {
                state.actions.clear();
                return Ok(ask(
                    "Provider batch stopped: predecessor was skipped or the observed layout changed",
                    started,
                ));
            }
        }
        if state.actions.is_empty() {
            let tools = if self.config.computer_tools_v1 {
                json!([{"type":"computer"}])
            } else {
                json!([{"type":"computer_use_preview","display_width":image.0,"display_height":image.1,"environment":self.config.environment}])
            };
            let input = if state.previous.is_some() {
                state
                    .calls
                    .iter()
                    .map(|id| {
                        let mut output =
                            cua_protocol::screenshot_output(id, &req.images.screenshot_jpeg);
                        if !self.config.computer_tools_v1 {
                            output["output"]["type"] = json!("input_image");
                        }
                        output
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![
                    json!({"role":"user","content":[{"type":"input_text","text":self.task},{"type":"input_image","image_url":format!("data:image/jpeg;base64,{}",base64::engine::general_purpose::STANDARD.encode(&req.images.screenshot_jpeg))}]}),
                ]
            };
            let mut body =
                json!({"model":self.config.model,"tools":tools,"truncation":"auto","input":input});
            if let Some(id) = &state.previous {
                body["previous_response_id"] = json!(id);
            }
            let response = match self.post(&body).await {
                Ok(v) => v,
                Err(e) => {
                    state.error = Some(e.to_string());
                    return Err(e);
                }
            };
            let decoded = match cua_protocol::parse_response(&response, observation, Some(1)) {
                Ok(v) => v,
                Err(e) => {
                    state.error = Some(e.to_string());
                    let detail = response["output"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|o| o["pending_safety_checks"].as_array().into_iter().flatten())
                        .filter_map(|c| c["message"].as_str())
                        .collect::<Vec<_>>()
                        .join("; ");
                    let why = if detail.is_empty() {
                        e.to_string()
                    } else {
                        format!("Safety check: {detail}")
                    };
                    state.message = Some(why.clone());
                    return Ok(ask(&why, started));
                }
            };
            let Some(id) = response["id"].as_str().filter(|id| !id.is_empty()) else {
                state.error = Some("Missing response id".into());
                return Ok(ask("Malformed provider response", started));
            };
            state.previous = Some(id.into());
            state.calls.clear();
            state.image = Some(image);
            state.message = decoded.message;
            let origin = self
                .eyes
                .observe_tree_only()
                .await
                .map_err(|_| ClientError::EmptyBody)?;
            let space = ObservationSpace {
                session: 1,
                id: observation,
                image_width: image.0,
                image_height: image.1,
                region: origin.screen.unwrap_or(crate::Rect::new(0., 0., 0., 0.)),
            };
            let mut requests = Vec::new();
            for call in &decoded.calls {
                for action in &call.actions {
                    requests.push(ToolRequest {
                        version: 1,
                        id: requests.len() as u64,
                        session: 1,
                        timeout_ms: 30_000,
                        action: action.clone(),
                    });
                }
            }
            if !requests.is_empty()
                && let Err(e) = validate_batch(&requests, &self.caps, &space)
            {
                state.error = Some(e.to_string());
                return Ok(ask(&e.to_string(), started));
            }
            state.origin_image = req.images.screenshot_jpeg.clone();
            state.origin_tree = Some(origin.grounding);
            for call in decoded.calls {
                state.calls.push(call.call_id);
                state.actions.extend(call.actions);
            }
            if state.actions.is_empty() {
                return Ok(ask(
                    state
                        .message
                        .as_deref()
                        .unwrap_or("The provider returned no actions; verify the task"),
                    started,
                ));
            }
        }
        let mut action = state.actions.pop_front().expect("nonempty validated queue");
        // Each ordered action receives a fresh observation and another host
        // policy check. Coordinates are preserved; dimensions cannot change.
        let refresh = |target: &mut Target| match target {
            Target::Point {
                observation: id, ..
            }
            | Target::Element {
                observation: id, ..
            } => *id = observation,
        };
        match &mut action {
            ComputerAction::Click { target, .. }
            | ComputerAction::Move { target }
            | ComputerAction::Scroll { target, .. } => refresh(target),
            ComputerAction::Drag { points, .. } => points.iter_mut().for_each(refresh),
            ComputerAction::Type { target, .. } => {
                let fresh = self
                    .eyes
                    .observe_tree_only()
                    .await
                    .map_err(|_| ClientError::EmptyBody)?;
                let Some(mark) = fresh.grounding.focused_field() else {
                    return Ok(ask("No confirmed focused editable field", started));
                };
                *target = Target::Element { observation, mark };
            }
            _ => {}
        }
        Ok(reply(
            json!({"thought":"provider computer action","action":"computer","computer":action,"risk":"high"}),
            started,
        ))
    }
}
