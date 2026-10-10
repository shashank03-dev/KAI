//! Several runs, one screen. Every task becomes a run in the sessions list
//! with its own transcript, frames and controls. Runs take the screen one at
//! a time: a single run cannot know in advance whether it will need the mouse
//! (any step may click), so every run queues behind the active one. Inside a
//! Team run, terminal sub-agents still run in parallel (the orchestrator's
//! design); GUI sub-agents take turns for the screen.

use crate::agent::Ui;
use crate::app::{KaiApp, Page};
use crate::control::RunControl;
use crate::state::{Phase, RunState};
use crate::{AgentHandle, mini};
use ely_gpui_component::{feedback::Toast, primitives::Severity};
use futures::StreamExt;
use gpui::{AppContext, Context, Entity, Window};
use kai_core::ApprovalChoice;
use std::sync::Arc;

/// One run in the sessions list.
pub struct RunEntry {
    pub id: u64,
    pub state: Entity<RunState>,
    pub control: Arc<RunControl>,
    /// Stopped from its own Stop button (not the app-wide kill switch).
    pub stopped_alone: bool,
}

/// A readable name for what a run uses: "Gemma 4 26B A4B · Gateway".
pub fn model_label(s: &kai_core::settings::Settings) -> String {
    if s.backend == kai_core::settings::BackendChoice::Local {
        return format!(
            "{} · {}",
            local_label(&s.local_model),
            backend_label(s.backend)
        );
    }
    let model = kai_core::settings::MODELS
        .iter()
        .find(|m| m.id == s.fast_model)
        .map(|m| m.label.to_string())
        .unwrap_or_else(|| s.fast_model.clone());
    format!("{model} · {}", backend_label(s.backend))
}

/// A local catalog id's label ("Gemma 4 E4B"), or the id itself.
pub fn local_label(id: &str) -> String {
    if id.is_empty() {
        return "No local model".into();
    }
    crate::local_api::load_catalog()
        .models
        .into_iter()
        .find(|m| m.id == id)
        .map(|m| m.label)
        .unwrap_or_else(|| id.to_string())
}

pub fn backend_label(b: kai_core::settings::BackendChoice) -> &'static str {
    use kai_core::settings::BackendChoice::*;
    match b {
        Both => "DigitalOcean + Gemini",
        Vllm => "DigitalOcean",
        Gemini => "Gemini API",
        Gateway => "AI Gateway",
        OpenAi => "OpenAI",
        Local => "This computer",
        Mock => "Mock",
    }
}

impl KaiApp {
    pub fn entry(&self, id: u64) -> Option<&RunEntry> {
        self.runs.iter().find(|r| r.id == id)
    }

    /// The run that has the screen now.
    pub fn active_state(&self) -> Option<Entity<RunState>> {
        self.active
            .and_then(|id| self.entry(id))
            .map(|e| e.state.clone())
    }

    pub fn queued_count(&self, cx: &gpui::App) -> usize {
        self.runs
            .iter()
            .filter(|r| r.state.read(cx).phase == Phase::Queued)
            .count()
    }

    /// Add a task to the queue (from the composer when `task` is None) and
    /// start it if the screen is free.
    pub fn start_run(&mut self, task: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let task = task.unwrap_or_else(|| self.draft.read(cx).text().trim().to_string());
        if task.is_empty() {
            return;
        }
        self.draft.update(cx, |d, cx| d.set_text("", cx));
        let (max_steps, _) = self.settings.limits();
        let (team, model) = (self.team, model_label(&self.settings));
        let state = cx.new(|_| RunState::default());
        state.update(cx, |r, _| r.queue(task, team, max_steps, model));
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        self.next_id += 1;
        let id = self.next_id;
        self.runs.push(RunEntry {
            id,
            state,
            control: Arc::new(RunControl::new(cx.global::<AgentHandle>().0.kill.clone())),
            stopped_alone: false,
        });
        self.select(Some(id), cx);
        if self.active.is_some() {
            self.toast(
                Toast::new("Queued")
                    .body("Kai starts it when the current run is done with the screen.")
                    .severity(Severity::Info),
                cx,
            );
        }
        self.pump(window, cx);
    }

    /// Start the next queued run if the screen is free.
    pub fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active.is_some() || self.queue_held {
            return;
        }
        let next = self
            .runs
            .iter()
            .find(|r| r.state.read(cx).phase == Phase::Queued)
            .map(|r| r.id);
        if let Some(id) = next {
            self.launch(id, window, cx);
        }
    }

    fn launch(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entry(id) else { return };
        let (state, control) = (entry.state.clone(), entry.control.clone());
        let mini_control = control.clone();
        let (task, team) = {
            let r = state.read(cx);
            (r.task.clone(), r.team)
        };
        state.update(cx, |r, cx| {
            r.begin();
            cx.notify();
        });
        self.active = Some(id);
        let agent = cx.global::<AgentHandle>().0.clone();
        let rx = agent.run(
            task,
            team,
            self.settings.clone(),
            kai_os::secrets::keys(),
            self.enabled_skills(),
            control,
        );

        // Get out of Kai's way: the mini window takes over, the main window steps aside.
        mini::open(state.clone(), mini_control, cx);
        if self.capture.is_none() {
            window.minimize_window();
        }
        mini::exclude_windows(&agent, cx);

        let main = window.window_handle().downcast::<KaiApp>();
        cx.spawn(async move |_, cx| {
            let mut rx = rx;
            while let Some(msg) = rx.next().await {
                state.update(cx, |r, cx| {
                    r.apply(msg);
                    cx.notify();
                });
            }
            // A run that ends without a result (the agent stopped) still ends.
            state.update(cx, |r, cx| {
                if r.is_live() {
                    r.apply(Ui::Failed("The run ended without a result.".into()));
                }
                cx.notify();
            });
            mini::close(cx);
            if let Some(main) = main {
                let _ = main.update(cx, |app, window, cx| {
                    app.finished(id, cx);
                    app.pump(window, cx);
                    if app.active.is_none() {
                        window.activate_window();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn finished(&mut self, id: u64, cx: &mut Context<Self>) {
        self.active = None;
        let agent = cx.global::<AgentHandle>().0.clone();
        let alone = self.entry(id).is_some_and(|e| e.stopped_alone);
        // The kill switch (hotkey, corner) stops everything: hold the queue so
        // the next run doesn't grab the mouse right after the user said stop.
        if agent.kill.is_tripped() && !alone && self.queued_count(cx) > 0 {
            self.queue_held = true;
        }
        let Some(state) = self.entry(id).map(|e| e.state.clone()) else {
            return;
        };
        let r = state.read(cx);
        let toast = match (&r.error, r.outcome()) {
            (Some(e), _) => Toast::new("Kai stopped")
                .body(e.clone())
                .severity(Severity::Danger),
            (None, Some(kai_core::Outcome::Success)) => {
                Toast::new("Done").severity(Severity::Success)
            }
            (None, Some(o)) => Toast::new(format!("Run ended: {o:?}")).severity(Severity::Warning),
            (None, None) => Toast::new("Run ended").severity(Severity::Info),
        };
        self.toast(toast, cx);
        if self.queue_held {
            self.toast(
                Toast::new("Queue paused")
                    .body("You stopped Kai, so the next task waits. Resume the queue in the sessions list.")
                    .severity(Severity::Warning),
                cx,
            );
        }
        self.pages.reload_history();
        cx.notify();
    }

    /// Stop everything: the kill switch, any pending approval, and the queue.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        cx.global::<AgentHandle>().0.stop();
        if self.queued_count(cx) > 0 {
            self.queue_held = true;
        }
        self.answer(ApprovalChoice::Stop, cx);
        cx.notify();
    }

    /// Stop one run: cancel it if queued, stop it within one step if running.
    pub fn stop_run(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(entry) = self.runs.iter_mut().find(|r| r.id == id) else {
            return;
        };
        let state = entry.state.clone();
        match state.read(cx).phase {
            Phase::Queued => state.update(cx, |r, cx| {
                r.cancel();
                cx.notify();
            }),
            Phase::Running | Phase::Waiting => {
                entry.stopped_alone = true;
                entry.control.stop();
                // Also interrupt typing or a click in progress.
                cx.global::<AgentHandle>().0.stop();
                if let Some(p) = state.update(cx, |r, _| r.approval.take()) {
                    let _ = p.reply.send(ApprovalChoice::Stop);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn set_paused(&mut self, id: u64, paused: bool, cx: &mut Context<Self>) {
        if let Some(e) = self.entry(id) {
            e.control.set_paused(paused);
            e.state.update(cx, |r, cx| {
                r.paused = paused;
                if !paused {
                    r.held = false;
                }
                cx.notify();
            });
        }
    }

    /// Run the same task again, as a new run at the end of the queue.
    pub fn retry(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some((task, team)) = self.entry(id).map(|e| {
            let r = e.state.read(cx);
            (r.task.clone(), r.team)
        }) else {
            return;
        };
        let keep = self.team;
        self.team = team;
        self.start_run(Some(task), window, cx);
        self.team = keep;
    }

    /// Send a message to a running agent; it reads it at its next step.
    pub fn steer(&mut self, id: u64, cx: &mut Context<Self>) {
        let text = self.steer_draft.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let Some(e) = self.entry(id) else { return };
        e.control.steer(text.clone());
        e.state.update(cx, |r, cx| {
            r.sent(text);
            cx.notify();
        });
        self.steer_draft.update(cx, |d, cx| d.set_text("", cx));
    }

    pub fn resume_queue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.queue_held = false;
        self.pump(window, cx);
        cx.notify();
    }

    /// Show a run (or, with None, the composer for a new task).
    pub fn select(&mut self, id: Option<u64>, cx: &mut Context<Self>) {
        self.selected = id.filter(|id| self.entry(*id).is_some());
        self.run = match self.selected.and_then(|id| self.entry(id)) {
            Some(e) => e.state.clone(),
            None => self.idle.clone(),
        };
        self.page = Page::Run;
        self.palette = false;
        cx.notify();
    }

    /// Drop finished runs from the list (their traces stay in History).
    pub fn clear_finished(&mut self, cx: &mut Context<Self>) {
        self.runs
            .retain(|r| r.state.read(cx).phase != Phase::Done || Some(r.id) == self.active);
        if self.selected.is_some_and(|id| self.entry(id).is_none()) {
            self.select(None, cx);
        }
        cx.notify();
    }

    /// Answer the active run's approval.
    pub fn answer(&mut self, choice: ApprovalChoice, cx: &mut Context<Self>) {
        let Some(state) = self.active_state() else {
            return;
        };
        if let Some(p) = state.update(cx, |r, cx| {
            if r.phase == Phase::Waiting {
                r.phase = Phase::Running;
            }
            cx.notify();
            r.approval.take()
        }) {
            let _ = p.reply.send(choice);
        }
    }
}
