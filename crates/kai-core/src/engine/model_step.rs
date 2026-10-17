//! One model-driven observe/decide/act round.
use super::*;
impl Engine<'_> {
    /// One observe → decide → act (→ batch) round. `Some` ends the run.
    pub(super) async fn model_step(
        &self,
        r: &mut Run<'_>,
        system: &str,
        schema: &serde_json::Value,
        active_skill: Option<&Skill>,
    ) -> Option<Outcome> {
        let step = r.step;
        self.drain_steering(r, step);
        r.exec_cooldown = r.exec_cooldown.saturating_sub(1);

        // ---- Observe + Ground (+ cheap verify of the last expect) ----
        let seen = match self.observe_step(r, false).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("observe failed: {e}");
                r.trace
                    .errors
                    .push(format!("observe error at step {step}: {e}"));
                r.step += 1;
                r.failure_streak += 1;
                return (r.failure_streak >= self.config.max_failures).then_some(Outcome::Failed);
            }
        };
        let obs = &seen.obs;
        if r.privacy.active() && matches!(obs.grounding, crate::Grounding::Grid { .. }) {
            return Some(Outcome::AskedUser);
        }
        let elements = obs.grounding.as_text();
        self.ledger_observe(r, step, obs);
        if r.trace.steps.is_empty() {
            self.completion_baseline(r, obs).await;
        }

        // Proven success is a hard stop: do not even dequeue the next native
        // computer action or ask the model to invent a follow-up click.
        if let Some(outcome) = self.media_complete(r, obs).await {
            return Some(outcome);
        }
        if let Some(outcome) = self.verified_goal_complete(r, obs).await {
            return Some(outcome);
        }

        // ---- Something only the user can get past (CAPTCHA, code, approval) ----
        if let Some(check) = self.unhandled_check(r, &obs.grounding) {
            return self.pause_for_human(r, check, None, &elements).await;
        }

        // ---- Decide ----
        let queued = self.next_queued(r, obs, step as u64 + 1);
        self.sink.emit(&EngineEvent::Thinking { step });
        // A queued action calls no model: its step leaves the notes for the next call.
        let mut notes = match queued {
            Some(_) => Vec::new(),
            None => std::mem::take(&mut r.notes),
        };
        notes.extend(r.plan_note.clone());
        if std::mem::take(&mut r.read_page) && !r.privacy.active() {
            let read = self.perceptor.page_text(crate::web::PAGE_TEXT_MAX).await;
            notes.push(crate::web::page_text_note(read));
        }
        if self.config.computer_tools_v1 {
            notes.push(format!("observation_id: {}", step as u64 + 1));
        }
        let first = self.outcome_request(r, &mut notes, schema);
        let lessons = self.lessons_for(r, obs);
        let verified = self.config.macro_history.then(|| self.verified_outcomes(r));
        let memo = r.memo.render();
        let user_text = prompt::build_turn(&TurnInput {
            lessons: lessons.as_deref(),
            task: r.task,
            grounding: &obs.grounding,
            history: &r.history,
            active_skill,
            no_progress: r.no_progress_streak >= 1,
            notes: &notes,
            has_screenshot: !obs.images.screenshot_jpeg.is_empty(),
            macro_history: verified.as_deref(),
            memo: memo.as_deref(),
        });
        let user_text = r.privacy.text(&user_text);
        let escalate = r.failure_streak > 0 || r.no_progress_streak > 0 || r.careful_next;
        r.careful_next = false;
        let (client, careful) = match (escalate, self.careful) {
            (true, Some(c)) => (c, true),
            _ => (self.client, false),
        };
        let (schema, extra) = first.as_ref().map_or((schema, 0), |(s, n)| (s, *n));
        let max_tokens = self.config.max_tokens + extra;
        let decided = match queued {
            Some(action) => self.queued_decision(client, action),
            None => {
                self.decide(
                    client,
                    system,
                    &user_text,
                    record::DecisionObservation {
                        obs,
                        id: step as u64 + 1,
                        result: r.computer_result.as_ref(),
                        recording: r.recorder.is_some(),
                    },
                    schema,
                    max_tokens,
                )
                .await
            }
        };
        if self.interrupt.cancelled() {
            return Some(Outcome::Stopped);
        }
        // Model inference may outlast the run deadline; never act on a late
        // reply, even if the action was queued by the provider.
        if r.out_of_time() {
            return Some(Outcome::TimeLimit);
        }
        r.step += 1;
        let mut decided = match decided {
            Ok(d) => d,
            Err(e) => {
                log::warn!("undecodable reply at step {step}: {e}");
                r.trace.errors.push(record::error_line(step, &e));
                r.failure_streak += 1;
                r.force_full = true;
                return (r.failure_streak >= self.config.max_failures)
                    .then_some(Outcome::AskedUser);
            }
        };
        r.failure_streak = 0;
        Self::queue_rest(r, obs, std::mem::take(&mut decided.queued));
        self.ledger_start(r, step, &decided.reply.outcomes);
        r.protect(&decided.reply.decision, &obs.grounding, false);
        let call = self.record_call(r, step, &user_text, obs, &decided, careful);
        let (reply, model_time, first_token) = (decided.reply, decided.total, decided.first_token);
        if reply.need_screen {
            r.force_full = true;
        }
        if reply.read_page && self.perceptor.reads_pages() {
            r.read_page = true;
        }
        if self.config.memo
            && let Some(what) = r.memo.update(reply.memo.as_deref())
        {
            log::warn!("memo refused at step {step}: it looks like {what}");
        }
        let decision = reply.decision;

        let mut timings = seen.timings;
        timings.set_model(model_time, first_token);
        let backend = client.backend().label();
        let mut rec = StepRecord::new(step, &decision, timings, backend);
        rec.elements = elements;
        rec.target = TargetSig::of(&decision, &obs.grounding);
        rec.screen = crate::memory::scope::screen_sig(obs.window_title.as_deref(), &obs.grounding);
        if let Some((shot, call)) = call {
            rec.screenshot = shot;
            rec.call = call;
        }

        if let Some(action) = reply.computer {
            return self.computer_step(r, action, obs, rec).await;
        }

        self.sink.emit(&EngineEvent::Decided {
            step,
            decision: Box::new(r.privacy.decision(&decision)),
            target: decision.target.and_then(|t| obs.grounding.target_rect(t)),
        });

        // ---- ask_user: a real question when someone can answer ----
        if decision.action == Action::AskUser && self.human.is_some() {
            let q = crate::human::HumanCheck::question(
                decision.text.as_deref().unwrap_or(&decision.thought),
            );
            return self.pause_for_human(r, q, Some(rec), "").await;
        }

        // ---- The model never works a CAPTCHA or guesses a one-time code ----
        if let Some(check) = crate::human::guard(&decision, &obs.grounding) {
            return self.refuse_for_human(r, check, rec).await;
        }

        // ---- done (through the gate) / ask_user terminate ----
        match decision.action {
            Action::Done => return self.claim_done(r, rec, obs).await,
            Action::AskUser => {
                rec.path = Some("none".into());
                r.push(rec);
                return Some(Outcome::AskedUser);
            }
            _ => {}
        }

        // ---- Approval for high-risk ----
        let final_risk = risk::classify(&decision, &obs.grounding);
        let pw = risk::is_payment_or_password(&decision, &obs.grounding);
        if risk::needs_approval(self.config.approval, final_risk, pw) {
            self.sink.emit(&EngineEvent::AwaitingApproval { step });
            match self.ask_approval(r, &decision, &obs.grounding).await {
                ApprovalChoice::Approve => rec.approval = Some("approved".to_string()),
                ApprovalChoice::Skip => {
                    rec.approval = Some("skipped".to_string());
                    rec.verified = false;
                    rec.path = Some("none".into());
                    r.push(rec);
                    r.history
                        .push(HistoryItem::note(format!("skipped {:?}", decision.action)));
                    r.pending = None;
                    return None;
                }
                ApprovalChoice::Stop => return Some(Outcome::Stopped),
            }
        }

        // ---- Act ----
        if self.interrupt.cancelled() {
            return Some(Outcome::Stopped);
        }
        if r.out_of_time() {
            return Some(Outcome::TimeLimit);
        }
        r.protect(&decision, &obs.grounding, false);
        let t_act = Instant::now();
        let guarded = act::reopen_guard(&mut r.opened, &decision)
            .or_else(|| act::repeat_guard(&r.history, &decision));
        let refused = guarded.is_some();
        r.events_mark = self.perceptor.ui_events();
        let acted = match guarded {
            Some(note) => Ok((Some(note), "none")),
            None => {
                self.act(&decision, &obs.grounding, &r.privacy, Some(obs))
                    .await
            }
        };
        let (output, path) = match acted {
            Ok((o, p)) => (o, p),
            Err(e) => {
                let e = super::act::act_error(r, &e);
                log::error!("act failed at step {step}: {e}");
                r.failure_streak += 1;
                (Some(e), "error")
            }
        };
        let output = output.map(|text| r.privacy.text(&text));
        rec.timings.act_ms = t_act.elapsed().as_millis() as u64;
        self.sink.emit(&EngineEvent::Acted {
            step,
            timings: rec.timings,
        });
        self.emit_output(step, &decision, output.as_deref());

        // ---- Verify (next observation checks `expect`) ----
        self.perceptor.settle(Duration::from_secs(3)).await;

        rec.path = Some(path.to_string());
        rec.output = output.clone();
        r.push(rec);
        let mut pending = Pending {
            summary: HistoryItem::from_decision(&decision).summary,
            action: decision.action,
            expect: decision.expect.clone(),
            typed: (decision.action == Action::Type)
                .then(|| decision.text.clone())
                .flatten(),
            keys: decision.keys.clone(),
            popup: observe::opens_popup(&decision, &obs.grounding),
            records: vec![r.trace.steps.len() - 1],
        };
        let mut item = HistoryItem::from_step(&decision, &obs.grounding);
        let acted_ok = path != "error";
        if let Some(out) = &output {
            item.push_output(out);
            // Terminal work changes no pixels; it is progress by itself.
            // A refused repeat is not: it must not keep a loop alive.
            if !refused {
                r.no_progress_streak = 0;
            }
        }
        if decision.action == Action::Wait {
            // Waiting is deliberate; an unchanged screen is expected.
            r.no_progress_streak = 0;
        }
        r.history.push(item);
        r.grid_click = observe::grid_click(&decision, &obs.grounding);

        // A reply may contain a 'then' batch or an executor plan. Check the
        // just-settled screen before either path can act again.
        if acted_ok && let Some(outcome) = self.completion_fresh(r).await {
            return Some(outcome);
        }

        // ---- Follow-ups the model was certain about ----
        if acted_ok
            && !reply.then.is_empty()
            && self.config.max_batch > 0
            && let Some(o) = self
                .run_batch(r, &reply.then, &obs.grounding, &mut pending)
                .await
        {
            return Some(o);
        }
        r.pending = Some(pending);

        if r.no_progress_streak >= 2 && !self.recover(r) {
            // Two steps with no change despite acting: stop and ask.
            return Some(Outcome::AskedUser);
        }
        // ---- The routine subgoals after this step: the executor ----
        if acted_ok && !reply.plan.is_empty() {
            return self.run_plan(r, &reply.plan).await;
        }
        None
    }
}
