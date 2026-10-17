//! Network adaptation only. All execution belongs to the shared Engine.
use super::turn::{self, Shot};
use super::{CuaConfig, jpeg_size};
use crate::client::{
    Backend, ClientError, ModelClient, ModelReply, ModelRequest, http_client, send_error,
};
use crate::cua_protocol::{self, ComputerResponse, Phase, SafetyCheck};
use crate::ports::{ApprovalChoice, Approver};
use crate::{Perceptor, computer::*};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::{collections::VecDeque, time::Instant};
use tokio::sync::Mutex;
mod formatting;
use formatting::{ask, reply};
/// Harness notes per run before a stopped batch goes to the user instead.
const MAX_NOTES: u32 = 2;
/// A completed commentary-only response can precede a tool call. Bound the
/// number of empty continuations so a stuck model cannot loop indefinitely.
const MAX_COMMENTARY: usize = 2;
struct QueuedAction {
    action: ComputerAction,
    /// The pre-conversion pixel delta, retained in the trace for auditing.
    scroll_pixels: Option<(i32, i32)>,
    /// Only the first action of a call presents its provider checks.
    provider_checks: Vec<SafetyCheck>,
    call_index: usize,
}
struct State {
    previous: Option<String>,
    /// Calls answered next turn, each with the safety checks the user accepted.
    calls: Vec<(String, Vec<SafetyCheck>)>,
    actions: VecDeque<QueuedAction>,
    /// The current batch's size and how many of it were handed out.
    batch: usize,
    dispatched: usize,
    notes: u32,
    message: Option<String>,
    error: Option<String>,
    image: Option<(u32, u32)>,
    origin_tree: Option<crate::Grounding>,
    domains: crate::safety_checks::RunDomains,
    pending_link_source: Option<String>,
}
pub(super) struct Provider<'a> {
    caps: Capabilities,
    config: &'a CuaConfig,
    task: &'a str,
    eyes: &'a dyn Perceptor,
    approver: &'a dyn Approver,
    key: String,
    http: reqwest::Client,
    state: Mutex<State>,
}
impl<'a> Provider<'a> {
    pub fn new(
        config: &'a CuaConfig,
        task: &'a str,
        eyes: &'a dyn Perceptor,
        approver: &'a dyn Approver,
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
            approver,
            key,
            http: http_client(),
            state: Mutex::new(State {
                previous: None,
                calls: vec![],
                actions: VecDeque::new(),
                batch: 0,
                dispatched: 0,
                notes: 0,
                message: None,
                error: None,
                image: None,
                origin_tree: None,
                domains: crate::safety_checks::RunDomains::new(task),
                pending_link_source: None,
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

    /// Read one complete decision, retrying an incomplete generation once
    /// with the exact same input. Syntax errors remain separate from network
    /// failures so the runner can ask the user about a malformed response.
    async fn exchange(
        &self,
        body: &Value,
        observation: u64,
    ) -> Result<Result<(Value, ComputerResponse), ToolError>, ClientError> {
        let mut response = self.post(body).await?;
        let mut decoded =
            match cua_protocol::parse_response_for_retry(&response, observation, Some(1)) {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            };
        if decoded.incomplete.is_some() {
            response = self.post(body).await?;
            decoded = match cua_protocol::parse_response_for_retry(&response, observation, Some(1))
            {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            };
        }
        if let Some(reason) = decoded.incomplete {
            return Err(ClientError::Incomplete(format!(
                "Provider response stayed incomplete ({reason}) after one retry; no actions ran"
            )));
        }
        Ok(Ok((response, decoded)))
    }
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
        let Some(seen) = jpeg_size(&req.images.screenshot_jpeg) else {
            return Ok(ask(
                "Computer use needs a safe screenshot; please continue manually",
                started,
            ));
        };
        if state.image.is_some_and(|previous| previous != seen) && !state.actions.is_empty() {
            return Ok(ask(
                "Display dimensions changed during the provider batch",
                started,
            ));
        }
        let mut note = None;
        if state.dispatched > 0 {
            let failed = req.computer_result.is_none_or(|r| r.error.is_some());
            if !failed {
                if let Some(source) = state.pending_link_source.take() {
                    state.domains.link_clicked(Some(&source));
                } else {
                    state.domains.abandon_link();
                }
            } else {
                state.pending_link_source = None;
                state.domains.abandon_link();
            }
            // Pixels always differ after an action (a caret, a hover), so
            // the layout decides if another queued action still has its place.
            // Even the final action needs its result checked: a denied final
            // action must not be reported to OpenAI as if it ran.
            let moved = if state.actions.is_empty() {
                false
            } else {
                let fresh = self
                    .eyes
                    .observe_tree_only()
                    .await
                    .map_err(|_| ClientError::EmptyBody)?;
                !state
                    .origin_tree
                    .as_ref()
                    .is_some_and(|t| crate::engine::fresh::same_layout(t, &fresh.grounding))
            };
            if failed || moved {
                let why = if failed {
                    "the last one was skipped or failed"
                } else {
                    "the screen layout changed"
                };
                let ran = state.dispatched - usize::from(failed);
                state.actions.clear();
                if state.notes >= MAX_NOTES {
                    return Ok(ask(&format!("Provider batch stopped: {why}"), started));
                }
                state.notes += 1;
                note = Some(crate::oai_contract::stopped_note(ran, state.batch, why));
            }
        }
        if state.actions.is_empty()
            && let Some(reply) = self
                .next_batch(&mut state, req, observation, seen, note)
                .await?
        {
            return Ok(reply);
        }
        let QueuedAction {
            mut action,
            scroll_pixels,
            provider_checks,
            call_index,
        } = state.actions.pop_front().expect("nonempty validated queue");
        state.dispatched += 1;
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
            | ComputerAction::Move { target, .. }
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
        let fresh = if self.config.local_safety_checks || !provider_checks.is_empty() {
            Some(
                self.eyes
                    .observe_tree_only()
                    .await
                    .map_err(|_| ClientError::EmptyBody)?,
            )
        } else {
            None
        };
        let (local, page_url) = match fresh.as_ref().filter(|_| self.config.local_safety_checks) {
            Some(obs) => (
                turn::local_safety(self.task, &mut state.domains, obs, &action, seen),
                match &action {
                    ComputerAction::OpenUrl { url } => Some(url.clone()),
                    _ => obs.url.clone(),
                },
            ),
            None => (Vec::new(), None),
        };
        let merged = crate::safety_checks::merge_native(provider_checks.clone(), local.clone());
        if !merged.is_empty() {
            let (flagged, decision) =
                turn::safety_decision(&merged, !provider_checks.is_empty(), !local.is_empty());
            if self
                .approver
                .approve_safety_check(
                    &decision,
                    &fresh.as_ref().expect("checks require tree").grounding,
                )
                .await
                != ApprovalChoice::Approve
            {
                let why = format!("{flagged} Not accepted, so nothing ran.");
                state.message = Some(why.clone());
                state.actions.clear();
                return Ok(ask(&why, started));
            }
            state.domains.accept(page_url.as_deref(), &local);
        }
        if !provider_checks.is_empty() {
            state.calls[call_index].1 = provider_checks;
        }
        state.pending_link_source = fresh
            .as_ref()
            .and_then(|obs| turn::observed_link_source(obs, &action, seen).map(str::to_owned));
        let thought = match (scroll_pixels, &action) {
            (Some((px, py)), ComputerAction::Scroll { dx, dy, .. }) => {
                format!("provider scroll: ({px}, {py}) pixels -> ({dx}, {dy}) wheel notches")
            }
            _ => "provider computer action".to_string(),
        };
        Ok(reply(
            json!({"thought":thought,"action":"computer","computer":action,"risk":"high"}),
            started,
        ))
    }
}

impl Provider<'_> {
    /// Ask the provider for the next batch and queue it, or return the
    /// reply that ends the turn (a question, a refusal, the final answer).
    async fn next_batch(
        &self,
        state: &mut State,
        req: &ModelRequest<'_>,
        observation: u64,
        seen: (u32, u32),
        note: Option<String>,
    ) -> Result<Option<ModelReply>, ClientError> {
        let started = Instant::now();
        let native = self.eyes.native_frame().await;
        let shot = match &native {
            Some(f) => Shot {
                image: &f.image,
                mime: f.mime,
                size: (f.width, f.height),
            },
            None => Shot {
                image: &req.images.screenshot_jpeg,
                mime: "image/jpeg",
                size: seen,
            },
        };
        let ga = self.config.computer_tools_v1;
        let input = match state.previous {
            Some(_) => turn::next_input(&state.calls, &shot, !ga, note.as_deref()),
            None => turn::first_input(self.task, &shot),
        };
        let mut body = turn::body(
            &self.config.model,
            input,
            state.previous.as_deref(),
            ga,
            shot.size,
            &self.config.environment,
            self.config.reasoning_effort,
        );
        let mut commentary_turns = 0;
        let (response, decoded) = loop {
            let (response, decoded) = match self.exchange(&body, observation).await {
                Ok(Ok(pair)) => pair,
                Ok(Err(e)) => {
                    state.error = Some(e.to_string());
                    state.message = Some(e.to_string());
                    return Ok(Some(ask(&e.to_string(), started)));
                }
                Err(e) => {
                    state.error = Some(e.to_string());
                    return Err(e);
                }
            };
            if decoded.calls.is_empty()
                && matches!(decoded.message.as_ref(), Some((_, Phase::Commentary)))
            {
                if commentary_turns >= MAX_COMMENTARY {
                    let why = "Provider returned only commentary; no computer actions";
                    state.message = Some(why.into());
                    return Ok(Some(ask(why, started)));
                }
                let Some(id) = response["id"].as_str().filter(|id| !id.is_empty()) else {
                    state.error = Some("Missing response id".into());
                    return Ok(Some(ask("Malformed provider response", started)));
                };
                // A commentary response completed without a tool call: it
                // has no call_id to acknowledge. Continue its stored history
                // with an empty input until the model acts or answers.
                body = turn::body(
                    &self.config.model,
                    Vec::new(),
                    Some(id),
                    ga,
                    shot.size,
                    &self.config.environment,
                    self.config.reasoning_effort,
                );
                commentary_turns += 1;
                continue;
            }
            break (response, decoded);
        };
        let Some(id) = response["id"].as_str().filter(|id| !id.is_empty()) else {
            state.error = Some("Missing response id".into());
            return Ok(Some(ask("Malformed provider response", started)));
        };
        state.previous = Some(id.into());
        state.image = Some(seen);
        state.message = decoded.message.map(|(text, _)| text);
        let origin = self
            .eyes
            .observe_tree_only()
            .await
            .map_err(|_| ClientError::EmptyBody)?;
        let space = ObservationSpace {
            session: 1,
            id: observation,
            image_width: seen.0,
            image_height: seen.1,
            region: origin.screen.unwrap_or(crate::Rect::new(0., 0., 0., 0.)),
        };
        let mut queued = Vec::new();
        for (call_index, call) in decoded.calls.iter().enumerate() {
            for (action_index, action) in call.actions.iter().enumerate() {
                let mut action = action.clone();
                let mut scroll_pixels = None;
                // Validate against the frame the model actually saw before
                // scaling. `rescale` bounds points to the destination frame,
                // so checking only afterwards would silently clamp invalid
                // source coordinates into a valid click.
                if action.targets().iter().any(|t| match t {
                    Target::Point { x, y, .. } => {
                        *x >= shot.size.0 as f64 || *y >= shot.size.1 as f64
                    }
                    Target::Element { .. } => false,
                }) {
                    state.error = Some("Provider action outside screenshot bounds".into());
                    return Ok(Some(ask(
                        "Provider action outside screenshot bounds",
                        started,
                    )));
                }
                crate::oai_contract::rescale(&mut action, shot.size, seen);
                // OpenAI uses pixel wheel deltas. Native desktops often only
                // advertise wheel notches; use OpenAI's documented conversion
                // after translating deltas into Kai's observation frame.
                if let ComputerAction::Scroll { dx, dy, unit, .. } = &mut action
                    && *unit == ScrollUnit::Pixels
                    && !self.caps.scroll_units.contains(&ScrollUnit::Pixels)
                    && self.caps.scroll_units.contains(&ScrollUnit::WheelNotches)
                {
                    scroll_pixels = Some((*dx, *dy));
                    *dx = pixels_to_notches(*dx);
                    *dy = pixels_to_notches(*dy);
                    *unit = ScrollUnit::WheelNotches;
                }
                queued.push(QueuedAction {
                    action,
                    scroll_pixels,
                    provider_checks: if action_index == 0 {
                        call.pending.clone()
                    } else {
                        vec![]
                    },
                    call_index,
                });
            }
        }
        let requests: Vec<ToolRequest> = queued
            .iter()
            .enumerate()
            .map(|(i, queued)| ToolRequest {
                version: 1,
                id: i as u64,
                session: 1,
                timeout_ms: 30_000,
                action: queued.action.clone(),
            })
            .collect();
        if !requests.is_empty()
            && let Err(e) = validate_batch(&requests, &self.caps, &space)
        {
            state.error = Some(e.to_string());
            return Ok(Some(ask(&e.to_string(), started)));
        }
        state.origin_tree = Some(origin.grounding);
        // Provider checks are added only after the person accepts each call.
        state.calls = decoded
            .calls
            .into_iter()
            .map(|c| (c.call_id, Vec::new()))
            .collect();
        state.batch = queued.len();
        state.dispatched = 0;
        state.actions.extend(queued);
        if state.actions.is_empty() {
            let why = state
                .message
                .as_deref()
                .unwrap_or("The provider returned no actions; verify the task");
            return Ok(Some(ask(why, started)));
        }
        Ok(None)
    }
}
