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
    /// Kai's own safety checks for this action's site: the person accepts
    /// them (never an auto-policy) or the action does not run, and the step
    /// ends with the returned outcome.
    async fn local_checks(
        &self,
        r: &mut Run<'_>,
        request: &ToolRequest,
        obs: &Observation,
        mut rec: StepRecord,
    ) -> Result<StepRecord, Option<Outcome>> {
        // Consume a link grant only after observing an actual new origin.
        r.domains.arrived(obs.url.as_deref());
        let url = match &request.action {
            ComputerAction::OpenUrl { url } => Some(url.as_str()),
            _ => obs.url.as_deref(),
        };
        let mut checks =
            crate::safety_checks::for_action(url, obs, &request.action, r.task, &r.domains);
        let watching = crate::safety_checks::watch(url, &obs.grounding)
            || crate::safety_checks::watch(obs.url.as_deref(), &obs.grounding);
        // Independent monitor only runs when the rule guard is silent and
        // the page has imperative text; its failure also needs human review.
        if !watching
            && !checks
                .iter()
                .any(|c| c.code.as_deref() == Some("malicious_instructions"))
            && let Some(model) = r.injection_monitor
        {
            let verdict = tokio::select! {
                biased;
                () = crate::ports::wait_cancelled(self.interrupt) => {
                    rec.approval = Some("safety monitor cancelled".into());
                    rec.path = Some("none".into());
                    r.push(rec);
                    return Err(Some(Outcome::Stopped));
                }
                check = super::injection_monitor::inspect(model, self.perceptor, obs, r.task) => check,
            };
            // A past approval cannot suppress a fresh flagged verdict.
            if let Some(check) = verdict {
                checks.push(check);
            }
        }
        if watching && checks.is_empty() {
            // Acknowledging a sensitive site is one-time, but every later
            // action still requires a present person (watch mode).
            checks.push(crate::safety_checks::watch_check(obs.screen_hash));
        }
        if checks.is_empty() {
            return Ok(rec);
        }
        // An unattended/Autopilot approver can auto-approve ordinary actions.
        // It must never silently accept provider-shaped safety checks.
        if self.config.approval == crate::risk::ApprovalLevel::Autopilot {
            rec.approval = Some("safety check needs a present person".into());
            rec.output = Some("A person must confirm the safety check before Kai can act.".into());
            rec.path = Some("none".into());
            r.push(rec);
            return Err(Some(Outcome::AskedUser));
        }
        let said: Vec<&str> = checks.iter().filter_map(|c| c.message.as_deref()).collect();
        let mut ask = rec.decision.clone();
        ask.thought = format!("Kai flagged this step: {} Allow it?", said.join(" "));
        self.sink
            .emit(&EngineEvent::AwaitingApproval { step: rec.index });
        let start = Instant::now();
        let choice = tokio::select! {
            biased;
            () = crate::ports::wait_cancelled(self.interrupt) => ApprovalChoice::Stop,
            c = self.approver.approve_safety_check(&ask, &obs.grounding) => c,
        };
        r.human.waited += start.elapsed();
        match choice {
            ApprovalChoice::Approve => {
                r.domains.accept(url, &checks);
                let codes = checks
                    .iter()
                    .filter_map(|c| c.code.as_deref())
                    .collect::<Vec<_>>();
                rec.approval = Some(format!("safety checks accepted: {}", codes.join(", ")));
                Ok(rec)
            }
            choice => {
                rec.approval = Some("safety check declined".into());
                rec.output = Some(said.join(" "));
                rec.path = Some("none".into());
                r.push(rec);
                Err(Some(match choice {
                    ApprovalChoice::Stop => Outcome::Stopped,
                    _ => Outcome::AskedUser,
                }))
            }
        }
    }

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
            return self.refused(r, rec, &request, e);
        }
        // Resolve every point/drag waypoint against known semantic elements.
        // A synthetic coordinate target must not hide a CAPTCHA/secure field.
        // A grounder chooses pixels without an accessibility identity. Even
        // Autopilot needs a person before committing such a click or drag.
        let mut mandatory = self.grounder.is_some()
            && matches!(obs.grounding, Grounding::Grid { .. })
            && matches!(
                request.action,
                ComputerAction::Click { .. } | ComputerAction::Drag { .. }
            )
            && request
                .action
                .targets()
                .into_iter()
                .any(|t| matches!(t, Target::Point { .. }));
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
                    return self.refused(r, rec, &request, ToolError::StaleObservation);
                }
                if let Some(el) = obs.grounding.element(mark) {
                    if !el.enabled {
                        return self.refused(r, rec, &request, ToolError::StaleObservation);
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
        if self.config.local_safety_checks || r.injection_monitor.is_some() {
            rec = match self.local_checks(r, &request, obs, rec).await {
                Ok(rec) => rec,
                Err(end) => return end,
            };
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
                ApprovalChoice::Approve => {
                    let previous = rec.approval.take();
                    rec.approval = Some(match previous {
                        Some(checks) => format!("{checks}; action approved"),
                        None => "approved".into(),
                    });
                }
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
        // One dispatcher per step on purpose: its replay guard is per request,
        // and the process-wide mutation lock (`MUTATION_OWNER`) is what
        // orders input across steps and runs (review CORE-ENG-10).
        let dispatcher = Dispatcher::new(&executor, &Authorized, self.interrupt);
        if r.out_of_time() {
            return Some(Outcome::TimeLimit);
        }
        let t = Instant::now();
        let result = dispatcher.dispatch(&request, &space).await;
        if result.error.is_none()
            && matches!(request.action, ComputerAction::Click { .. })
            && request.action.targets().iter().any(|target| match target {
                Target::Element { mark, .. } => obs
                    .grounding
                    .element(*mark)
                    .is_some_and(|e| e.role == crate::element::Role::Link),
                _ => false,
            })
        {
            r.domains.link_clicked(obs.url.as_deref());
        } else {
            r.domains.abandon_link();
        }
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
        // Native and provider-shaped computer tools bypass the legacy act
        // path. Confirm completion here, including on the final allowed step,
        // before any queued action or another model turn can run.
        if result.error.is_none()
            && let Some(outcome) = self.completion_fresh(r).await
        {
            return Some(outcome);
        }
        match result.error {
            None => {
                r.rejected = 0;
                None
            }
            Some(e) => end_for(e, &mut r.rejected),
        }
    }

    /// A computer action that cannot run as given (invalid, or aimed at a
    /// stale or disabled element): the model hears why and gets a fresh look,
    /// instead of the run ending at once (review CORE-ENG-02).
    fn refused(
        &self,
        r: &mut Run<'_>,
        mut rec: StepRecord,
        request: &ToolRequest,
        e: ToolError,
    ) -> Option<Outcome> {
        r.computer_result = Some(ToolResult {
            id: request.id,
            session: request.session,
            error: Some(e),
            elapsed_ms: 0,
            requires_observation: true,
        });
        rec.output = Some(e.to_string());
        rec.path = Some("error".into());
        r.notes.push(format!(
            "ERROR: that computer action was refused ({e}); look again and retry."
        ));
        r.push(rec);
        r.force_full = true;
        end_for(e, &mut r.rejected)
    }
}

/// Refusals in a row before the user is asked.
pub(super) const MAX_REJECTED: u32 = 3;

/// How a computer-tool error ends the step: errors the model can recover
/// from go back to it (until [`MAX_REJECTED`] in a row); a person is needed
/// for permission and human checks; a stop stops.
fn end_for(e: ToolError, rejected: &mut u32) -> Option<Outcome> {
    match e {
        ToolError::Cancelled => Some(Outcome::Stopped),
        ToolError::NeedsUser | ToolError::PermissionDenied => Some(Outcome::AskedUser),
        _ => {
            *rejected += 1;
            (*rejected >= MAX_REJECTED).then_some(Outcome::AskedUser)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recoverable_tool_errors_go_back_to_the_model_until_repeated() {
        let mut n = 0;
        assert_eq!(end_for(ToolError::StaleObservation, &mut n), None);
        assert_eq!(end_for(ToolError::InvalidArguments, &mut n), None);
        assert_eq!(
            end_for(ToolError::Unsupported, &mut n),
            Some(Outcome::AskedUser)
        );
        let mut n = 0;
        assert_eq!(
            end_for(ToolError::NeedsUser, &mut n),
            Some(Outcome::AskedUser)
        );
        assert_eq!(
            end_for(ToolError::Cancelled, &mut n),
            Some(Outcome::Stopped)
        );
    }
}
