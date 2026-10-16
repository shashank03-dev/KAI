//! Plan-level best-of-N before the first action (`crate::best_of`).

use super::{Engine, Run};
use crate::best_of::{self, MAX_SAMPLES, MAX_TOKENS, TEMPERATURE};
use crate::client::{Images, ModelRequest};

impl Engine<'_> {
    /// For a long task with `Config::best_of` ≥ 2: ask for that many plans in
    /// parallel, let the judge pick one, and keep it as a note for every
    /// turn. Any failure just means no plan.
    pub(super) async fn best_of_plan(&self, r: &mut Run<'_>) {
        let n = self.config.best_of.min(MAX_SAMPLES);
        if n < 2 || !best_of::is_long(r.task) {
            return;
        }
        let mut obs = match self.perceptor.observe_tree_only().await {
            Ok(o) => o,
            Err(e) => {
                log::info!("best-of: no plans, reading the screen failed: {e}");
                return;
            }
        };
        r.redact(&mut obs.grounding);
        let text = best_of::user_text(r.task, &obs.grounding.as_compact_text());
        let schema = best_of::schema();
        let images = Images {
            screenshot_jpeg: Vec::new(),
            crop_jpeg: None,
        };
        let req = ModelRequest {
            observation_id: None,
            computer_result: None,
            system: best_of::SYSTEM,
            user_text: &text,
            images: &images,
            schema: &schema,
            max_tokens: MAX_TOKENS,
            temperature: TEMPERATURE,
        };
        let replies = futures::future::join_all((0..n).map(|_| self.client.decide(&req))).await;
        let replies: Vec<_> = replies
            .into_iter()
            .filter_map(|x| {
                x.map_err(|e| log::info!("best-of: a plan call failed: {e}"))
                    .ok()
            })
            .collect();
        let sum = |f: fn(&crate::client::ModelReply) -> Option<u32>| {
            replies.iter().filter_map(f).reduce(|a, b| a + b)
        };
        let (prompt_tokens, completion_tokens) =
            (sum(|r| r.prompt_tokens), sum(|r| r.completion_tokens));
        let plans: Vec<Vec<String>> = replies
            .iter()
            .filter_map(|reply| best_of::parse(&reply.raw))
            .collect();
        if let Some(mut b) = best_of::judge(plans, n) {
            (b.prompt_tokens, b.completion_tokens) = (prompt_tokens, completion_tokens);
            log::info!(
                "best-of: plan {} of {} chosen (scores {:?})",
                b.chosen + 1,
                b.plans.len(),
                b.scores
            );
            r.plan_note = Some(best_of::note(&b));
            r.trace.best_of = Some(b);
        }
    }
}
