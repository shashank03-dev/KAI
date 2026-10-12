//! Sub-agents. One planner call splits a task into subtasks; each runs as its
//! own [`Engine`] with its own goal. Terminal and research subtasks run
//! concurrently; GUI subtasks share the one screen and mouse, so they take a
//! lock and run one at a time. A final call writes the report.
//!
//! The planner is skipped for simple tasks: one GUI subtask is just a normal run.

use crate::client::{Images, ModelClient, ModelRequest};
use crate::element::Grounding;
use crate::engine::{
    Actuator, Approver, Config, Engine, EventSink, Interrupt, Observation, Perceptor,
};
use crate::skill::Skill;
use crate::trace::{Outcome, Trace};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, Semaphore};

/// Where a subtask runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Needs the screen, mouse and keyboard. Serialised.
    Gui,
    /// Terminal only (commands, scans, files). Runs in parallel.
    Shell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subtask {
    pub id: String,
    pub goal: String,
    pub kind: Kind,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub subtasks: Vec<Subtask>,
}

const MAX_SUBTASKS: usize = 8;

impl Plan {
    pub fn json_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object", "additionalProperties": false, "required": ["subtasks"],
            "properties": { "subtasks": { "type": "array", "maxItems": MAX_SUBTASKS, "items": {
                "type": "object", "additionalProperties": false,
                "required": ["id", "goal", "kind"],
                "properties": {
                    "id": { "type": "string" },
                    "goal": { "type": "string" },
                    "kind": { "type": "string", "enum": ["gui", "shell"] },
                    "depends_on": { "type": "array", "items": { "type": "string" } }
                }
            }}}
        })
    }

    /// Parse and sanity-check a planner reply. Unknown dependencies are
    /// dropped, cycles are broken, and the plan is capped.
    pub fn parse(raw: &str) -> anyhow::Result<Plan> {
        let start = raw
            .find('{')
            .ok_or_else(|| anyhow::anyhow!("no JSON in plan"))?;
        let end = raw
            .rfind('}')
            .ok_or_else(|| anyhow::anyhow!("no JSON in plan"))?;
        let mut plan: Plan = serde_json::from_str(&raw[start..=end])?;
        plan.subtasks.truncate(MAX_SUBTASKS);
        plan.subtasks.retain(|s| !s.goal.trim().is_empty());
        if plan.subtasks.is_empty() {
            anyhow::bail!("planner returned no subtasks");
        }
        let ids: Vec<String> = plan.subtasks.iter().map(|s| s.id.clone()).collect();
        // Keep only dependencies on earlier subtasks: no unknown ids, no cycles.
        for (i, s) in plan.subtasks.iter_mut().enumerate() {
            s.depends_on.retain(|d| ids[..i].contains(d));
        }
        Ok(plan)
    }

    /// Group subtasks into waves: each wave depends only on earlier waves.
    pub fn waves(&self) -> Vec<Vec<&Subtask>> {
        let mut done: Vec<&str> = Vec::new();
        let mut left: Vec<&Subtask> = self.subtasks.iter().collect();
        let mut waves = Vec::new();
        while !left.is_empty() {
            let (ready, rest): (Vec<&Subtask>, Vec<&Subtask>) = left
                .into_iter()
                .partition(|s| s.depends_on.iter().all(|d| done.contains(&d.as_str())));
            // Parse guarantees progress; this guard keeps a bad plan from looping.
            let ready = if ready.is_empty() {
                rest.clone()
            } else {
                ready
            };
            let rest: Vec<&Subtask> = rest.into_iter().filter(|s| !ready.contains(s)).collect();
            done.extend(ready.iter().map(|s| s.id.as_str()));
            waves.push(ready);
            left = rest;
        }
        waves
    }
}

/// The result of one sub-agent.
#[derive(Debug, Clone)]
pub struct SubResult {
    pub subtask: Subtask,
    pub trace: Trace,
}

impl SubResult {
    /// Everything the sub-agent learned, for dependents and the final report.
    pub fn findings(&self) -> String {
        let mut s = String::new();
        for st in &self.trace.steps {
            if let Some(out) = &st.output {
                s.push_str(&format!(
                    "$ {}\n{}\n",
                    st.decision.text.as_deref().unwrap_or(""),
                    out
                ));
            }
            if matches!(
                st.decision.action,
                crate::schema::Action::Done | crate::schema::Action::AskUser
            ) && !st.decision.thought.is_empty()
            {
                s.push_str(&format!("Conclusion: {}\n", st.decision.thought));
            }
        }
        s
    }
}

/// A perceptor for terminal-only sub-agents: no screenshot, no elements.
pub struct Headless {
    tick: AtomicU64,
}

impl Default for Headless {
    fn default() -> Self {
        Self {
            tick: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl Perceptor for Headless {
    async fn observe(&self) -> anyhow::Result<Observation> {
        Ok(Observation {
            images: Images {
                screenshot_jpeg: Vec::new(),
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(Vec::new()),
            screen_hash: self.tick.fetch_add(1, Ordering::SeqCst),
            timing: Default::default(),
            screen: None,
            window_title: None,
            url: None,
        })
    }
}

/// Runs a task as a team of sub-agents.
pub struct Orchestrator<'a> {
    pub client: &'a dyn ModelClient,
    pub careful: Option<&'a dyn ModelClient>,
    pub screen: &'a dyn Perceptor,
    pub actuator: &'a dyn Actuator,
    pub approver: &'a dyn Approver,
    pub interrupt: &'a dyn Interrupt,
    pub sink: &'a dyn EventSink,
    /// User messages mid-run; whichever sub-agent steps next reads them.
    pub steering: &'a dyn crate::engine::Steering,
    /// Who to ask at a CAPTCHA, one-time code or question (any sub-agent).
    pub human: Option<&'a dyn crate::human::HumanHelper>,
    pub skills: &'a [Skill],
    pub config: Config,
    /// How many terminal sub-agents may run at once.
    pub parallelism: usize,
}

/// The whole team's result.
pub struct Report {
    pub plan: Plan,
    pub results: Vec<SubResult>,
    pub summary: String,
}

impl Report {
    pub fn outcome(&self) -> Outcome {
        if self
            .results
            .iter()
            .any(|r| r.trace.outcome == Some(Outcome::Stopped))
        {
            Outcome::Stopped
        } else if self
            .results
            .iter()
            .all(|r| r.trace.outcome == Some(Outcome::Success))
        {
            Outcome::Success
        } else {
            Outcome::Failed
        }
    }
}

const PLANNER: &str = "You split a computer task into independent subtasks for parallel sub-agents. \
Use kind `shell` for anything doable in a terminal (commands, scans, file work, network lookups) \
and `gui` only when the screen is needed. Make subtasks independent so they run in parallel; use \
depends_on only when one truly needs another's result. Use 1 subtask for a simple task. At most 8.";

const SUMMARY: &str = "You are Kai. Write a short, clear report for the user from the sub-agent \
findings: what was done, what was found (most important first), and suggested next steps. \
Plain text, no JSON.";

impl Orchestrator<'_> {
    pub async fn plan(&self, task: &str) -> anyhow::Result<Plan> {
        let schema = Plan::json_schema();
        let images = Images {
            screenshot_jpeg: Vec::new(),
            crop_jpeg: None,
        };
        let req = ModelRequest {
            system: PLANNER,
            user_text: task,
            images: &images,
            schema: &schema,
            max_tokens: 768,
            temperature: 0.0,
        };
        Plan::parse(&self.client.decide(&req).await?.raw)
    }

    pub async fn run(&self, task: &str) -> anyhow::Result<Report> {
        let plan = self.plan(task).await.unwrap_or_else(|e| {
            log::warn!("planner failed ({e}); running as one GUI task");
            Plan {
                subtasks: vec![Subtask {
                    id: "1".into(),
                    goal: task.into(),
                    kind: Kind::Gui,
                    depends_on: vec![],
                }],
            }
        });
        self.run_plan(task, plan).await
    }

    /// Run an already-made plan (e.g. the one the UI showed), with no second
    /// planning call.
    pub async fn run_plan(&self, task: &str, plan: Plan) -> anyhow::Result<Report> {
        let gui_lock = Mutex::new(());
        let slots = Semaphore::new(self.parallelism.max(1));
        let headless = Headless::default();
        let mut results: HashMap<String, SubResult> = HashMap::new();

        for wave in plan.waves() {
            if self.interrupt.stopped() {
                break;
            }
            let futs = wave.into_iter().map(|sub| {
                let context: String = sub
                    .depends_on
                    .iter()
                    .filter_map(|d| results.get(d))
                    .map(|r| format!("[{}] {}\n{}", r.subtask.id, r.subtask.goal, r.findings()))
                    .collect();
                let (gui_lock, slots, headless) = (&gui_lock, &slots, &headless);
                async move {
                    let goal = if context.is_empty() {
                        sub.goal.clone()
                    } else {
                        format!(
                            "{}\n\nResults from earlier sub-agents:\n{context}",
                            sub.goal
                        )
                    };
                    let trace = match sub.kind {
                        Kind::Gui => {
                            let _screen = gui_lock.lock().await;
                            self.engine(self.screen).run(&goal).await
                        }
                        Kind::Shell => {
                            let _slot = slots.acquire().await.expect("semaphore open");
                            self.engine(headless).run(&goal).await
                        }
                    };
                    SubResult {
                        subtask: sub.clone(),
                        trace,
                    }
                }
            });
            for r in futures::future::join_all(futs).await {
                results.insert(r.subtask.id.clone(), r);
            }
        }

        let ordered: Vec<SubResult> = plan
            .subtasks
            .iter()
            .filter_map(|s| results.remove(&s.id))
            .collect();
        let summary = self.summarise(task, &ordered).await;
        Ok(Report {
            plan,
            results: ordered,
            summary,
        })
    }

    fn engine<'b>(&'b self, perceptor: &'b dyn Perceptor) -> Engine<'b> {
        Engine {
            client: self.client,
            careful: self.careful,
            perceptor,
            actuator: self.actuator,
            approver: self.approver,
            interrupt: self.interrupt,
            sink: self.sink,
            steering: self.steering,
            verifier: None,
            human: self.human,
            skills: self.skills,
            // Sub-agents run without the outcome ledger (no probe port here;
            // docs/VERIFICATION.md).
            config: Config {
                outcomes: false,
                ..self.config
            },
        }
    }

    async fn summarise(&self, task: &str, results: &[SubResult]) -> String {
        let mut text = format!("Task: {task}\n\n");
        for r in results {
            text.push_str(&format!(
                "## Sub-agent {} ({:?}): {}\nOutcome: {:?}\n{}\n",
                r.subtask.id,
                r.subtask.kind,
                r.subtask.goal,
                r.trace.outcome,
                r.findings()
            ));
        }
        let images = Images {
            screenshot_jpeg: Vec::new(),
            crop_jpeg: None,
        };
        let req = ModelRequest {
            system: SUMMARY,
            user_text: &text,
            images: &images,
            schema: &serde_json::Value::Null,
            max_tokens: 1024,
            temperature: 0.2,
        };
        match self.client.decide(&req).await {
            Ok(r) => r.raw,
            Err(e) => {
                log::warn!("summary call failed: {e}");
                text
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(id: &str, deps: &[&str]) -> Subtask {
        Subtask {
            id: id.into(),
            goal: format!("goal {id}"),
            kind: Kind::Shell,
            depends_on: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    #[test]
    fn independent_subtasks_form_one_wave() {
        let p = Plan {
            subtasks: vec![st("a", &[]), st("b", &[]), st("c", &[])],
        };
        assert_eq!(p.waves().len(), 1);
        assert_eq!(p.waves()[0].len(), 3);
    }

    #[test]
    fn dependencies_make_later_waves() {
        let p = Plan {
            subtasks: vec![st("a", &[]), st("b", &[]), st("c", &["a", "b"])],
        };
        let w = p.waves();
        assert_eq!(w.len(), 2);
        assert_eq!(w[1][0].id, "c");
    }

    #[test]
    fn parse_drops_unknown_and_forward_deps() {
        let raw = r#"{"subtasks":[
            {"id":"a","goal":"x","kind":"shell","depends_on":["b","zzz"]},
            {"id":"b","goal":"y","kind":"gui","depends_on":["a"]}]}"#;
        let p = Plan::parse(raw).unwrap();
        assert!(
            p.subtasks[0].depends_on.is_empty(),
            "forward/unknown deps dropped"
        );
        assert_eq!(p.subtasks[1].depends_on, vec!["a"]);
    }

    #[test]
    fn parse_caps_and_rejects_empty() {
        assert!(Plan::parse(r#"{"subtasks":[]}"#).is_err());
        let many: Vec<String> = (0..20)
            .map(|i| format!(r#"{{"id":"{i}","goal":"g","kind":"shell"}}"#))
            .collect();
        let raw = format!(r#"{{"subtasks":[{}]}}"#, many.join(","));
        assert_eq!(Plan::parse(&raw).unwrap().subtasks.len(), MAX_SUBTASKS);
    }
}
