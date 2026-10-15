//! Experience memory in the loop: recall lessons into the user turn (again
//! whenever a new app or site comes into play) and learn from the run at the
//! end. Lessons are text for the model only: approvals, risk and the kill
//! switch never read them. See `docs/MEMORY.md`.

use super::{Engine, Run};
use crate::client::{Images, ModelRequest};
use crate::memory::recall::{self, Context};
use crate::memory::{MemoryStore, learn, reflect, scope};
use crate::ports::Observation;
use crate::trace::Trace;

/// What recall has seen this run.
#[derive(Default)]
pub(super) struct Recalled {
    /// Apps and sites in play so far (they only grow during a run, so the
    /// block stays stable and cache-friendly).
    apps: Vec<String>,
    domains: Vec<String>,
    /// The context key of the last recall.
    key: Option<String>,
    /// The block shown in every turn until the key changes.
    block: Option<String>,
}

fn add_all(into: &mut Vec<String>, from: Vec<String>) -> bool {
    let mut grew = false;
    for x in from {
        if !into.contains(&x) {
            into.push(x);
            grew = true;
        }
    }
    grew
}

impl Engine<'_> {
    /// The lesson block for this step's user turn, if any.
    pub(super) fn lessons_for(&self, r: &mut Run<'_>, obs: &Observation) -> Option<String> {
        let g = &obs.grounding;
        let store = r.memory?;
        let (apps, domains) = scope::of_steps(&r.trace);
        let text = g.as_text();
        let known = store.apps();
        add_all(&mut r.mem.apps, apps);
        add_all(&mut r.mem.apps, scope::apps_in(&text, &known));
        add_all(&mut r.mem.domains, domains);
        add_all(&mut r.mem.domains, scope::domains_in(&text));
        let cx = Context::new(r.task, &r.mem.apps, &r.mem.domains)
            .at_screen(scope::screen_sig(obs.window_title.as_deref(), g));
        let key = cx.key();
        if r.mem.key.as_deref() != Some(key.as_str()) {
            let lessons = store.recall_in(&cx);
            for l in &lessons {
                if !r.trace.lessons.contains(&l.id) {
                    r.trace.lessons.push(l.id.clone());
                }
            }
            let block = recall::render(&lessons);
            if block != r.mem.block {
                log::info!(
                    "memory: {} lessons for apps {:?} sites {:?}",
                    lessons.len(),
                    cx.apps,
                    cx.domains
                );
            }
            r.mem.key = Some(key);
            r.mem.block = block;
        }
        r.mem.block.clone()
    }

    /// Learn from a finished run: the optional reflection call, then credit
    /// and the deterministic rules ([`MemoryStore::after_run`]).
    pub(super) async fn learn_from(&self, store: &MemoryStore, trace: &Trace) {
        if !self.config.learn {
            return;
        }
        let extra = if self.config.reflect && learn::worth_reflecting(trace, None) {
            self.reflect(trace).await
        } else {
            Vec::new()
        };
        match store.after_run(trace, None, extra, true) {
            Ok(a) => {
                for l in &a.learned {
                    log::info!("memory: learned [{}] {}", l.kind.as_str(), l.text);
                }
            }
            Err(e) => log::warn!("memory: could not save lessons: {e}"),
        }
    }

    /// One extra model call asking for at most two lessons.
    async fn reflect(&self, trace: &Trace) -> Vec<crate::memory::Lesson> {
        let summary = reflect::summary(trace, None);
        let schema = reflect::schema();
        let images = Images {
            screenshot_jpeg: Vec::new(),
            crop_jpeg: None,
        };
        let req = ModelRequest {
            observation_id: None,
            computer_result: None,
            system: reflect::SYSTEM,
            user_text: &summary,
            images: &images,
            schema: &schema,
            max_tokens: reflect::MAX_TOKENS,
            temperature: 0.0,
        };
        match self.client.decide(&req).await {
            Ok(reply) => reflect::parse(&reply.raw, trace, None),
            Err(e) => {
                log::warn!("memory: reflection call failed: {e}");
                Vec::new()
            }
        }
    }
}
