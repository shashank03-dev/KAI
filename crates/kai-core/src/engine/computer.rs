//! The shared tools enter the existing policy and trace boundaries.
use super::{Engine, Pending, Run};
use crate::computer::*;
use crate::ports::{ApprovalChoice, EngineEvent};
use crate::trace::StepRecord;
use crate::{Action, Decision, Grounding, Observation, Outcome, Risk};
use async_trait::async_trait;
use std::time::{Duration, Instant};

struct Executor<'a> {
    engine: &'a Engine<'a>,
    obs: &'a Observation,
    privacy: &'a super::privacy::Privacy,
}
#[async_trait]
impl ComputerExecutor for Executor<'_> {
    fn capabilities(&self) -> Capabilities {
        self.engine.actuator.computer_capabilities()
    }
    async fn is_current(
        &self,
        a: &ComputerAction,
        _: &ObservationSpace,
    ) -> Result<bool, ToolError> {
        let mut fresh = if !self.obs.images.screenshot_jpeg.is_empty() {
            self.engine.perceptor.observe().await
        } else {
            self.engine.perceptor.observe_tree_only().await
        }
        .map_err(|_| ToolError::Failed)?;
        self.privacy.sanitise(&mut fresh);
        let mut expected = self.obs.grounding.clone();
        self.privacy.redact(&mut expected);
        // The action's target and the foreground scope must still match;
        // pixels only where a grid gives nothing else to compare.
        let grid = matches!(expected, Grounding::Grid { .. });
        Ok((!grid || fresh.screen_hash == self.obs.screen_hash)
            && fresh.screen == self.obs.screen
            && super::fresh::same_tool_target(&expected, &fresh.grounding, a)
            && fresh.window_title
                == self
                    .obs
                    .window_title
                    .as_deref()
                    .map(|s| self.privacy.text(s))
            && fresh.url == self.obs.url.as_deref().map(|s| self.privacy.text(s)))
    }
    async fn execute(
        &self,
        a: &ComputerAction,
        s: &ObservationSpace,
        c: &ExecutionContext,
    ) -> Result<(), ToolError> {
        self.engine
            .actuator
            .computer_act(a, s, &self.obs.grounding, c)
            .await
    }
}
struct Authorized;
#[async_trait]
impl ComputerPolicy for Authorized {
    async fn authorize(&self, _: &ToolRequest) -> Result<(), ToolError> {
        Ok(())
    }
}

/// A conservative policy decision; the model's declared risk cannot lower it.
fn policy(action: &ComputerAction, original: &Decision, g: &Grounding) -> Decision {
    let mut d = original.clone();
    d.risk = Risk::High;
    d.text = None;
    d.target = None;
    let target = match action {
        ComputerAction::Click { target, .. }
        | ComputerAction::Move { target, .. }
        | ComputerAction::Type { target, .. }
        | ComputerAction::Scroll { target, .. } => Some(target),
        ComputerAction::Drag { points, .. } => points.first(),
        _ => None,
    };
    if let Some(Target::Element { mark, .. }) = target {
        d.target = Some(*mark);
    }
    // Coordinate mutations have no trustworthy semantic identity; always ask.
    match action {
        ComputerAction::Observe | ComputerAction::Wait { .. } | ComputerAction::Move { .. } => {
            d.action = Action::Wait;
            d.risk = Risk::None;
        }
        ComputerAction::Click { .. } | ComputerAction::Drag { .. } => d.action = Action::Click,
        ComputerAction::Type { text, .. } => {
            d.action = Action::Type;
            d.text = Some(text.clone());
        }
        ComputerAction::Keypress { keys } => {
            d.action = Action::Key;
            d.keys = keys.clone();
        }
        ComputerAction::Scroll { .. } => {
            d.action = Action::Scroll;
            d.risk = Risk::Low;
        }
        ComputerAction::OpenApp { name } => {
            d.action = Action::OpenApp;
            d.text = Some(name.clone());
        }
        ComputerAction::OpenUrl { url } => {
            d.action = Action::OpenUrl;
            d.text = Some(url.clone());
        }
    }
    d.risk = crate::risk::classify(&d, g);
    d
}
impl Engine<'_> {
    pub(super) async fn computer_step(
        &self,
        r: &mut Run<'_>,
        action: ComputerAction,
        obs: &Observation,
        mut rec: StepRecord,
    ) -> Option<Outcome> {
        let mut d = policy(&action, &rec.decision, &obs.grounding);
        let (w, h) = crate::cua::jpeg_size(&obs.images.screenshot_jpeg).unwrap_or((0, 0));
        let space = ObservationSpace {
            session: r.session,
            id: rec.index as u64 + 1,
            image_width: w,
            image_height: h,
            region: obs.screen.unwrap_or(crate::Rect::new(0., 0., 0., 0.)),
        };
        let request = ToolRequest {
            version: 1,
            id: space.id,
            session: space.session,
            timeout_ms: 30_000,
            action,
        };
        if let Err(e) = request.validate(&self.actuator.computer_capabilities(), &space) {
            rec.output = Some(e.to_string());
            rec.path = Some("error".into());
            r.push(rec);
            return Some(Outcome::AskedUser);
        }
        // Resolve every point/drag waypoint against known semantic elements.
        // A synthetic coordinate target must not hide a CAPTCHA/secure field.
        let mut mandatory = false;
        for target in request.action.targets() {
            let marks = match target {
                Target::Element { mark, .. } => vec![*mark],
                Target::Point { x, y, .. } => {
                    let (x, y) = space.point(*x, *y).expect("validated coordinates");
                    match &obs.grounding {
                        Grounding::Marks(els) => els
                            .iter()
                            .filter(|el| {
                                let b = el.bounds;
                                x >= b.x as f64
                                    && y >= b.y as f64
                                    && x < (b.x + b.w) as f64
                                    && y < (b.y + b.h) as f64
                            })
                            .map(|el| el.mark)
                            .collect(),
                        _ => vec![],
                    }
                }
            };
            for mark in marks {
                if matches!(obs.grounding, Grounding::Marks(_))
                    && obs.grounding.element(mark).is_none()
                {
                    rec.output = Some(ToolError::StaleObservation.to_string());
                    r.push(rec);
                    return Some(Outcome::AskedUser);
                }
                if let Some(el) = obs.grounding.element(mark) {
                    if !el.enabled {
                        rec.output = Some(ToolError::StaleObservation.to_string());
                        r.push(rec);
                        return Some(Outcome::AskedUser);
                    }
                    let mut probe = d.clone();
                    probe.target = Some(mark);
                    // Moves/scrolls do not submit input, but never manipulate a human check.
                    if !matches!(request.action, ComputerAction::Type { .. }) {
                        probe.action = Action::Click;
                    }
                    if let Some(check) = crate::human::guard(&probe, &obs.grounding) {
                        return self.refuse_for_human(r, check, rec).await;
                    }
                    mandatory |= crate::risk::is_payment_or_password(&probe, &obs.grounding);
                }
            }
        }
        r.protect(&d, &obs.grounding, mandatory);
        d.risk = crate::risk::classify(&d, &obs.grounding);
        rec.decision.risk = d.risk;
        if crate::risk::needs_approval(
            self.config.approval,
            d.risk,
            mandatory || crate::risk::is_payment_or_password(&d, &obs.grounding),
        ) {
            self.sink
                .emit(&EngineEvent::AwaitingApproval { step: rec.index });
            // Show the exact canonical payload, rather than its policy projection.
            match self.ask_approval(r, &rec.decision, &obs.grounding).await {
                ApprovalChoice::Approve => rec.approval = Some("approved".into()),
                ApprovalChoice::Skip => {
                    r.computer_result = Some(ToolResult {
                        id: request.id,
                        session: request.session,
                        error: Some(ToolError::PermissionDenied),
                        elapsed_ms: 0,
                        requires_observation: false,
                    });
                    rec.approval = Some("skipped".into());
                    rec.path = Some("none".into());
                    r.push(rec);
                    return None;
                }
                ApprovalChoice::Stop => return Some(Outcome::Stopped),
            }
        }
        r.protect(
            &d,
            &obs.grounding,
            crate::risk::is_payment_or_password(&d, &obs.grounding),
        );
        // Clear the call which could contain unescaped secret JSON variants.
        if r.privacy.active() {
            rec.decision.text = Some("[sensitive computer action]".into());
            rec.call.reply_raw = None;
            rec.call.prompt_user = None;
        }
        self.sink.emit(&EngineEvent::Decided {
            step: rec.index,
            decision: Box::new(rec.decision.clone()),
            target: d.target.and_then(|m| obs.grounding.target_rect(m)),
        });
        let executor = Executor {
            engine: self,
            obs,
            privacy: &r.privacy,
        };
        let dispatcher = Dispatcher::new(&executor, &Authorized, self.interrupt);
        let t = Instant::now();
        let result = dispatcher.dispatch(&request, &space).await;
        r.computer_result = Some(result.clone());
        rec.timings.act_ms = t.elapsed().as_millis() as u64;
        rec.path = Some(
            if result.error.is_none() {
                "computer"
            } else {
                "error"
            }
            .into(),
        );
        rec.output = Some(serde_json::to_string(&result).expect("result serializes"));
        r.notes.push(rec.output.clone().unwrap());
        let index = r.trace.steps.len();
        r.pending = Some(Pending {
            summary: format!("computer {:?}", request.action.kind()),
            action: Action::Computer,
            expect: d.expect,
            typed: None,
            keys: Vec::new(),
            popup: false,
            records: vec![index],
        });
        r.push(rec);
        r.force_full = true;
        self.perceptor.settle(Duration::from_secs(3)).await;
        match result.error {
            Some(ToolError::Cancelled) => Some(Outcome::Stopped),
            Some(_) => Some(Outcome::AskedUser),
            None => None,
        }
    }
}
