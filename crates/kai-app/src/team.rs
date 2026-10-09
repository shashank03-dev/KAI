//! What a run keeps beside its steps: the terminal log, the user's steering
//! messages, and in team mode each sub-agent's status.
//!
//! Sub-agent events carry no sub-agent id (the orchestrator shares one event
//! sink), so live status is derived from the plan's waves: sub-agents in a
//! wave run together, and each one that ends sends one `Finished` event. Once
//! as many have finished as the earlier waves hold, the next wave is running.
//! Exact outcomes per sub-agent arrive with the team's report at the end.

use crate::agent::SubOutcome;
use kai_core::Outcome;
use kai_core::orchestrator::{Kind, Plan};

/// A terminal command Kai ran.
#[derive(Debug, Clone)]
pub struct OutputView {
    pub step: u32,
    pub command: String,
    pub output: String,
}

/// A message the user sent to the running agent.
#[derive(Debug, Clone)]
pub struct SteerView {
    pub text: String,
    /// The step that was running when it was sent.
    pub sent_at: u32,
    /// The step the agent read it in, once it has.
    pub read_at: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubState {
    /// Waiting for earlier sub-agents.
    Waiting,
    /// Running (GUI sub-agents may be waiting their turn for the screen).
    Running,
    /// Ended; the outcome comes with the report.
    Finished,
    Ended(Outcome),
}

#[derive(Debug, Clone)]
pub struct SubView {
    pub id: String,
    pub goal: String,
    pub kind: Kind,
    pub wave: usize,
    pub state: SubState,
    pub steps: Option<usize>,
}

/// The team's sub-agents and how far they got.
#[derive(Debug, Clone, Default)]
pub struct Team {
    pub subs: Vec<SubView>,
    finished: usize,
}

impl Team {
    pub fn from_plan(plan: &Plan) -> Self {
        let mut subs = Vec::new();
        for (wave, members) in plan.waves().into_iter().enumerate() {
            for s in members {
                subs.push(SubView {
                    id: s.id.clone(),
                    goal: s.goal.clone(),
                    kind: s.kind,
                    wave,
                    state: SubState::Waiting,
                    steps: None,
                });
            }
        }
        let mut team = Self { subs, finished: 0 };
        team.update();
        team
    }

    /// One sub-agent's engine finished.
    pub fn one_finished(&mut self) {
        self.finished += 1;
        self.update();
    }

    fn update(&mut self) {
        let waves = self.subs.iter().map(|s| s.wave + 1).max().unwrap_or(0);
        let mut before = 0;
        for w in 0..waves {
            let size = self.subs.iter().filter(|s| s.wave == w).count();
            let state = if self.finished >= before + size {
                SubState::Finished
            } else if self.finished >= before {
                SubState::Running
            } else {
                SubState::Waiting
            };
            for s in self.subs.iter_mut().filter(|s| s.wave == w) {
                if !matches!(s.state, SubState::Ended(_)) {
                    s.state = state;
                }
            }
            before += size;
        }
    }

    /// The report: the plan that actually ran and how each sub-agent ended.
    pub fn report(&mut self, plan: &Plan, results: &[SubOutcome]) {
        *self = Self::from_plan(plan);
        for s in &mut self.subs {
            match results.iter().find(|r| r.id == s.id) {
                Some(r) => {
                    s.state = r.outcome.map_or(SubState::Finished, SubState::Ended);
                    s.steps = Some(r.steps);
                }
                // Never started (the run was stopped first).
                None => s.state = SubState::Waiting,
            }
        }
    }

    /// The whole run stopped or failed: nothing is running any more.
    pub fn settle(&mut self) {
        for s in &mut self.subs {
            if s.state == SubState::Running {
                s.state = SubState::Finished;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kai_core::orchestrator::Subtask;

    fn plan() -> Plan {
        let st = |id: &str, deps: &[&str]| Subtask {
            id: id.into(),
            goal: format!("goal {id}"),
            kind: Kind::Shell,
            depends_on: deps.iter().map(|d| d.to_string()).collect(),
        };
        Plan {
            subtasks: vec![st("a", &[]), st("b", &[]), st("c", &["a", "b"])],
        }
    }

    #[test]
    fn waves_advance_as_sub_agents_finish() {
        let mut t = Team::from_plan(&plan());
        let states = |t: &Team| t.subs.iter().map(|s| s.state).collect::<Vec<_>>();
        use SubState::*;
        assert_eq!(states(&t), [Running, Running, Waiting]);
        t.one_finished();
        assert_eq!(states(&t), [Running, Running, Waiting]);
        t.one_finished();
        assert_eq!(states(&t), [Finished, Finished, Running]);
        t.one_finished();
        assert_eq!(states(&t), [Finished, Finished, Finished]);
    }

    #[test]
    fn the_report_sets_exact_outcomes() {
        let mut t = Team::from_plan(&plan());
        t.report(
            &plan(),
            &[
                SubOutcome {
                    id: "a".into(),
                    outcome: Some(Outcome::Success),
                    steps: 3,
                },
                SubOutcome {
                    id: "b".into(),
                    outcome: Some(Outcome::Failed),
                    steps: 5,
                },
            ],
        );
        assert_eq!(t.subs[0].state, SubState::Ended(Outcome::Success));
        assert_eq!(t.subs[1].steps, Some(5));
        assert_eq!(t.subs[2].state, SubState::Waiting, "c never ran");
    }
}
