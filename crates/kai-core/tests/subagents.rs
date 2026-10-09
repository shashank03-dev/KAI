//! Sub-agents: a plan of three terminal subtasks runs concurrently, and a
//! dependent subtask sees its parents' output.

use async_trait::async_trait;
use kai_core::client::{Backend, ClientError, ModelReply, ModelRequest};
use kai_core::engine::{Actuator, ApprovalChoice, Approver, NullSink};
use kai_core::orchestrator::{Headless, Kind, Orchestrator};
use kai_core::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Answers by what it is asked, so concurrent sub-agents stay deterministic.
struct Router {
    seen_context: AtomicUsize,
}

#[async_trait]
impl ModelClient for Router {
    fn backend(&self) -> Backend {
        Backend::Mock
    }
    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        let raw = if req.system.contains("split a computer task") {
            r#"{"subtasks":[
              {"id":"dns","goal":"look up DNS for example.com","kind":"shell"},
              {"id":"tls","goal":"check TLS for example.com","kind":"shell"},
              {"id":"hdr","goal":"check headers for example.com","kind":"shell"},
              {"id":"sum","goal":"combine the findings","kind":"shell","depends_on":["dns","tls","hdr"]}]}"#
                .to_string()
        } else if req.schema.is_null() {
            "REPORT".to_string()
        } else if req.user_text.contains("output:") {
            r#"{"thought":"finished","action":"done","risk":"none"}"#.to_string()
        } else {
            if req.user_text.contains("Results from earlier sub-agents")
                && req.user_text.contains("$ echo")
            {
                self.seen_context.fetch_add(1, Ordering::SeqCst);
            }
            r#"{"thought":"run it","action":"shell","text":"echo hi","risk":"low"}"#.to_string()
        };
        Ok(ModelReply {
            raw,
            prompt_tokens: None,
            completion_tokens: None,
            first_token: None,
            total: Duration::ZERO,
            backend: Backend::Mock,
            cost_usd: None,
        })
    }
}

#[derive(Default)]
struct Term {
    now: AtomicUsize,
    peak: AtomicUsize,
    runs: Mutex<Vec<String>>,
}

#[async_trait]
impl Actuator for Term {
    async fn act(&self, _: &Decision, _: &Grounding) -> anyhow::Result<()> {
        Ok(())
    }
    async fn open_app(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn open_url(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn shell(&self, cmd: &str) -> anyhow::Result<String> {
        let n = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(n, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(50)).await;
        self.now.fetch_sub(1, Ordering::SeqCst);
        self.runs.lock().unwrap().push(cmd.to_string());
        Ok("hi".into())
    }
}

struct Yes;
#[async_trait]
impl Approver for Yes {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        ApprovalChoice::Approve
    }
}

#[tokio::test]
async fn shell_subagents_run_in_parallel_and_feed_dependents() {
    let client = Router {
        seen_context: AtomicUsize::new(0),
    };
    let term = Term::default();
    let screen = Headless::default();
    let orch = Orchestrator {
        client: &client,
        careful: None,
        screen: &screen,
        actuator: &term,
        approver: &Yes,
        interrupt: &kai_core::engine::NeverStops,
        sink: &NullSink,
        steering: &kai_core::engine::NoSteering,
        skills: &[],
        config: Config::default(),
        parallelism: 4,
    };
    let report = orch.run("audit example.com").await.unwrap();

    assert_eq!(report.plan.subtasks.len(), 4);
    assert!(report.plan.subtasks.iter().all(|s| s.kind == Kind::Shell));
    assert_eq!(report.results.len(), 4);
    assert_eq!(report.outcome(), Outcome::Success);
    assert_eq!(report.summary, "REPORT");
    assert!(
        term.peak.load(Ordering::SeqCst) >= 3,
        "first wave should overlap"
    );
    assert_eq!(
        client.seen_context.load(Ordering::SeqCst),
        1,
        "dependent saw parents' output"
    );
}
