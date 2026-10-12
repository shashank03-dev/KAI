//! OpenAI's native computer-use loop, as a comparison runner beside Kai's own
//! harness (labelled [`Backend::OpenAiNative`]).
//!
//! Responses API (`POST /v1/responses`, model `computer-use-preview`, tool
//! `computer_use_preview`, `truncation: "auto"`): send the task and a
//! screenshot, loop on `computer_call` items, act, reply with a
//! `computer_call_output` holding the next screenshot. `pending_safety_checks`
//! stop the run and ask the user. The model clicks raw pixels; they are mapped
//! from screenshot space to screen space and handed to the existing
//! [`Actuator`] as a one-element grounding, so `kai-os` needs no changes.
//!
//! Note: the screenshot is [`Observation::images`]' `screenshot_jpeg`, which
//! carries Kai's Set-of-Marks boxes. The native model ignores the numbers but
//! sees the overlay; a clean frame would need a `Perceptor` option.

use crate::client::{Backend, ClientError, http_client, send_error};
use crate::element::{Element, Grounding, Rect, Role};
use crate::ports::{Actuator, EngineEvent, EventSink, Interrupt, Observation, Perceptor};
use crate::risk;
use crate::schema::Decision;
use crate::trace::{Outcome, StepRecord, Timings, Trace};
use base64::Engine as _;
use std::time::{Duration, Instant};

/// Settings for one native run.
#[derive(Debug, Clone)]
pub struct CuaConfig {
    /// e.g. `https://api.openai.com/v1`.
    pub base: String,
    /// `OPENAI_API_KEY`. Required: the AI Gateway does not serve
    /// `computer-use-preview` (it answers 404 model_not_found, measured).
    pub api_key: Option<String>,
    pub model: String,
    /// `linux` or `windows`.
    pub environment: String,
    pub max_steps: u32,
    pub max_time: Duration,
    pub timeout: Duration,
}

impl Default for CuaConfig {
    fn default() -> Self {
        Self {
            base: crate::client::OPENAI_BASE.to_string(),
            api_key: None,
            model: "computer-use-preview".to_string(),
            environment: if cfg!(windows) { "windows" } else { "linux" }.to_string(),
            max_steps: 40,
            max_time: Duration::from_secs(600),
            timeout: Duration::from_secs(60),
        }
    }
}

/// What a native run produced: the trace (scored like Kai's), and the error
/// that ended it early, if any.
#[derive(Debug)]
pub struct CuaRun {
    pub trace: Trace,
    pub error: Option<String>,
    /// The model's last text message (its summary or question), if any.
    pub message: Option<String>,
}

/// The native loop's collaborators.
pub struct CuaRunner<'a> {
    pub config: CuaConfig,
    pub perceptor: &'a dyn Perceptor,
    pub actuator: &'a dyn Actuator,
    pub interrupt: &'a dyn Interrupt,
    pub sink: &'a dyn EventSink,
}

impl CuaRunner<'_> {
    /// Run `task` with OpenAI's own loop. Fails fast with a clear error when no
    /// OpenAI key is set; any later API error ends the run with
    /// [`Outcome::Failed`] and is returned in [`CuaRun::error`].
    pub async fn run(&self, task: &str) -> Result<CuaRun, ClientError> {
        let key = self
            .config
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
            .ok_or(ClientError::MissingKey(
                "OPENAI_API_KEY (OpenAI's native computer-use model is only on api.openai.com)",
            ))?;
        let http = http_client();
        let mut trace = Trace::new(task);
        let mut message = None;
        let deadline = Instant::now() + self.config.max_time;
        let label = Backend::OpenAiNative.label().to_string();

        let mut obs = match self.perceptor.observe().await {
            Ok(o) => o,
            Err(e) => return Ok(self.end(trace, Outcome::Failed, Some(e.to_string()), None)),
        };
        let (w, h) = jpeg_size(&obs.images.screenshot_jpeg).unwrap_or((1280, 720));
        let tools = serde_json::json!([{
            "type": "computer_use_preview",
            "display_width": w,
            "display_height": h,
            "environment": self.config.environment,
        }]);
        let mut body = serde_json::json!({
            "model": self.config.model,
            "tools": tools,
            "truncation": "auto",
            "reasoning": { "summary": "concise" },
            "input": [{
                "role": "user",
                "content": [
                    { "type": "input_text", "text": task },
                    { "type": "input_image", "image_url": data_url(&obs.images.screenshot_jpeg) }
                ]
            }]
        });

        for step in 0..self.config.max_steps {
            if self.interrupt.stopped() {
                return Ok(self.end(trace, Outcome::Stopped, None, message));
            }
            if Instant::now() > deadline {
                return Ok(self.end(trace, Outcome::TimeLimit, None, message));
            }
            self.sink.emit(&EngineEvent::Thinking { step });
            let t_model = Instant::now();
            let resp = match self.post(&http, &key, &body).await {
                Ok(v) => v,
                Err(e) => {
                    return Ok(self.end(trace, Outcome::Failed, Some(e.to_string()), message));
                }
            };
            let model_time = t_model.elapsed();
            let output = resp["output"].as_array().cloned().unwrap_or_default();
            if let Some(m) = last_text(&output) {
                message = Some(m);
            }
            let Some(call) = output.iter().find(|o| o["type"] == "computer_call") else {
                // No more actions: the model considers the task done.
                return Ok(self.end(trace, Outcome::Success, None, message));
            };
            let checks = call["pending_safety_checks"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let screen = obs
                .screen
                .unwrap_or(Rect::new(0.0, 0.0, w as f32, h as f32));
            let mapped = if checks.is_empty() {
                match map_action(&call["action"], (w, h), screen) {
                    // Native typing lands wherever focus is; a focused
                    // password field (by Kai's own tree read) is the user's.
                    Mapped::Act(d, _) if risk::is_payment_or_password(&d, &obs.grounding) => {
                        Mapped::Ask("Typing into a password or secure field needs you.".into())
                    }
                    m => m,
                }
            } else {
                let why: Vec<&str> = checks
                    .iter()
                    .filter_map(|c| c["message"].as_str())
                    .collect();
                Mapped::Ask(format!("Safety check: {}", why.join("; ")))
            };
            let mut timings = Timings {
                capture_ms: obs.timing.capture_ms,
                tree_ms: obs.timing.tree_ms,
                encode_ms: obs.timing.encode_ms,
                ..Default::default()
            };
            timings.set_model(model_time, None);
            let elements = obs.grounding.as_text();
            let (decision, grounding) = match mapped {
                Mapped::Act(d, g) => (d, g),
                Mapped::Ask(why) => {
                    let d = decision(serde_json::json!({
                        "thought": why, "action": "ask_user", "text": why, "risk": "high"
                    }));
                    trace.push(record(step, d, timings, &label, elements, false));
                    return Ok(self.end(trace, Outcome::AskedUser, None, Some(why)));
                }
            };
            self.sink.emit(&EngineEvent::Decided {
                step,
                decision: Box::new(decision.clone()),
                target: decision.target.and_then(|t| grounding.target_rect(t)),
            });
            let t_act = Instant::now();
            let acted = match decision.action {
                crate::schema::Action::Wait => {
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    Ok(())
                }
                _ => self.actuator.act(&decision, &grounding).await,
            };
            timings.act_ms = t_act.elapsed().as_millis() as u64;
            self.sink.emit(&EngineEvent::Acted { step, timings });
            trace.push(record(
                step,
                decision,
                timings,
                &label,
                elements,
                acted.is_ok(),
            ));
            if let Err(e) = acted {
                log::warn!("native step {step} could not act: {e}");
            }
            self.perceptor.settle(Duration::from_millis(1500)).await;
            obs = match self.perceptor.observe().await {
                Ok(o) => o,
                Err(e) => {
                    return Ok(self.end(trace, Outcome::Failed, Some(e.to_string()), message));
                }
            };
            self.observed(step + 1, &obs);
            body = serde_json::json!({
                "model": self.config.model,
                "previous_response_id": resp["id"],
                "tools": tools,
                "truncation": "auto",
                "input": [{
                    "type": "computer_call_output",
                    "call_id": call["call_id"],
                    "output": {
                        "type": "input_image",
                        "image_url": data_url(&obs.images.screenshot_jpeg)
                    }
                }]
            });
        }
        Ok(self.end(trace, Outcome::StepLimit, None, message))
    }

    async fn post(
        &self,
        http: &reqwest::Client,
        key: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ClientError> {
        let resp = http
            .post(format!(
                "{}/responses",
                self.config.base.trim_end_matches('/')
            ))
            .bearer_auth(key)
            .json(body)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| send_error(e, self.config.timeout))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ClientError::from_status(status.as_u16(), &text));
        }
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| ClientError::EmptyBody)?;
        if v["error"].is_object() {
            return Err(ClientError::Status(500, v["error"].to_string()));
        }
        Ok(v)
    }

    fn observed(&self, step: u32, obs: &Observation) {
        self.sink.emit(&EngineEvent::Observed {
            step,
            elements: obs.grounding.as_text(),
            screenshot: obs.images.screenshot_jpeg.clone().into(),
            screen: obs.screen,
        });
    }

    fn end(
        &self,
        mut trace: Trace,
        o: Outcome,
        err: Option<String>,
        msg: Option<String>,
    ) -> CuaRun {
        trace.finish(o);
        self.sink.emit(&EngineEvent::Finished { outcome: o });
        CuaRun {
            trace,
            error: err,
            message: msg,
        }
    }
}

fn record(i: u32, d: Decision, t: Timings, label: &str, elements: String, ok: bool) -> StepRecord {
    StepRecord {
        index: i,
        decision: d,
        timings: t,
        backend: label.to_string(),
        elements,
        screenshot: None,
        approval: None,
        verified: ok,
        output: None,
        path: Some("native".to_string()),
        call: Default::default(),
        executor: None,
        effect: None,
        target: None,
        screen: None,
    }
}

/// What a native action becomes.
pub enum Mapped {
    Act(Decision, Grounding),
    Ask(String),
}

/// Build a [`Decision`] from JSON, so new optional fields keep their defaults.
fn decision(v: serde_json::Value) -> Decision {
    let mut d: Decision = serde_json::from_value(v).expect("well-formed decision");
    let _ = d.validate_and_clamp();
    d
}

/// Map a `computer_call.action` to a Kai decision plus a one-element grounding
/// at the global point. `img` is the screenshot size the model saw; `screen`
/// the captured display in global pixels.
pub fn map_action(a: &serde_json::Value, img: (u32, u32), screen: Rect) -> Mapped {
    let point = |x: &serde_json::Value, y: &serde_json::Value| {
        let sx = screen.w / img.0.max(1) as f32;
        let sy = screen.h / img.1.max(1) as f32;
        let gx = screen.x + x.as_f64().unwrap_or(0.0) as f32 * sx;
        let gy = screen.y + y.as_f64().unwrap_or(0.0) as f32 * sy;
        Grounding::Marks(vec![Element {
            mark: 1,
            role: Role::Other,
            name: "native pointer".into(),
            bounds: Rect::new(gx - 0.5, gy - 0.5, 1.0, 1.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }])
    };
    let none = || Grounding::Marks(Vec::new());
    let ty = a["type"].as_str().unwrap_or("");
    let at = || point(&a["x"], &a["y"]);
    let act = |v: serde_json::Value, g: Grounding| Mapped::Act(decision(v), g);
    match ty {
        "click" => match a["button"].as_str().unwrap_or("left") {
            "right" => act(
                serde_json::json!({"thought":"native right click","action":"right_click","target":1,"risk":"none"}),
                at(),
            ),
            "back" => act(
                serde_json::json!({"thought":"native back","action":"key","keys":["alt","left"],"risk":"none"}),
                none(),
            ),
            "forward" => act(
                serde_json::json!({"thought":"native forward","action":"key","keys":["alt","right"],"risk":"none"}),
                none(),
            ),
            _ => act(
                serde_json::json!({"thought":"native click","action":"click","target":1,"risk":"none"}),
                at(),
            ),
        },
        "double_click" => act(
            serde_json::json!({"thought":"native double click","action":"double_click","target":1,"risk":"none"}),
            at(),
        ),
        "scroll" => {
            let dx = a["scroll_x"].as_f64().unwrap_or(0.0);
            let dy = a["scroll_y"].as_f64().unwrap_or(0.0);
            let (dir, d) = if dy.abs() >= dx.abs() {
                (if dy < 0.0 { "up" } else { "down" }, dy.abs())
            } else {
                (if dx < 0.0 { "left" } else { "right" }, dx.abs())
            };
            let amount = ((d / 100.0).ceil() as u32).clamp(1, 20);
            act(
                serde_json::json!({"thought":"native scroll","action":"scroll","target":1,"direction":dir,"amount":amount,"risk":"none"}),
                at(),
            )
        }
        "type" => act(
            serde_json::json!({"thought":"native type","action":"type","text":a["text"],"risk":"none"}),
            none(),
        ),
        "keypress" => {
            let keys: Vec<String> = a["keys"]
                .as_array()
                .map(|k| k.iter().filter_map(|s| s.as_str()).map(key_name).collect())
                .unwrap_or_default();
            if keys.is_empty() {
                return act(
                    serde_json::json!({"thought":"native keypress (empty)","action":"wait","risk":"none"}),
                    none(),
                );
            }
            act(
                serde_json::json!({"thought":"native keypress","action":"key","keys":keys,"risk":"none"}),
                none(),
            )
        }
        "wait" | "screenshot" => act(
            serde_json::json!({"thought":format!("native {ty}"),"action":"wait","risk":"none"}),
            none(),
        ),
        // The Actuator has no bare move or drag: record them and carry on.
        "move" | "drag" => act(
            serde_json::json!({"thought":format!("native {ty} (not supported by the actuator; skipped)"),"action":"wait","risk":"none"}),
            none(),
        ),
        other => Mapped::Ask(format!("Unknown native action `{other}`")),
    }
}

/// CUA key names (`CTRL`, `ARROWLEFT`, `ENTER`) to Kai's.
fn key_name(k: &str) -> String {
    match k.to_ascii_lowercase().as_str() {
        "arrowleft" => "left".into(),
        "arrowright" => "right".into(),
        "arrowup" => "up".into(),
        "arrowdown" => "down".into(),
        "control" => "ctrl".into(),
        "super" | "meta" | "win" | "cmd" => "cmd".into(),
        " " => "space".into(),
        other => other.to_string(),
    }
}

fn last_text(output: &[serde_json::Value]) -> Option<String> {
    output
        .iter()
        .filter(|o| o["type"] == "message")
        .flat_map(|o| o["content"].as_array().cloned().unwrap_or_default())
        .filter_map(|c| c["text"].as_str().map(str::to_string))
        .next_back()
}

fn data_url(jpeg: &[u8]) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    )
}

/// Width and height from a JPEG's SOF marker.
pub fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if (0xC0..=0xCF).contains(&m) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}
