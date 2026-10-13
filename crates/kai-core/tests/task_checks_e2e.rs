//! Task checks through the real engine (`task_checks.rs`): a `done` before
//! the task's named output exists is refused, the model is told why, and
//! with the switch off nothing changes.

mod common;

use async_trait::async_trait;
use common::{Counting, Hands, Screen, approve_all, engine, flag};
use kai_core::probe::{ProbeSpec, ProbeVerdict};
use kai_core::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const TASK: &str = "Write just the price into kai-bench/price.txt in my home folder.";
const DONE: &str = r#"{"thought":"finished","action":"done","risk":"none"}"#;
const CLICK: &str =
    r#"{"thought":"save","action":"click","target":2,"risk":"none","expect":"saved"}"#;

/// The file appears after `ready_after` checks of it.
struct Disk {
    checks: AtomicUsize,
    ready_after: usize,
}

#[async_trait]
impl engine::Probe for Disk {
    async fn check(&self, p: &ProbeSpec) -> anyhow::Result<ProbeVerdict> {
        let ProbeSpec::FileNotEmpty { path } = p else {
            anyhow::bail!("unexpected probe {}", p.label());
        };
        assert_eq!(path, "~/kai-bench/price.txt");
        let n = self.checks.fetch_add(1, Ordering::SeqCst);
        Ok(if n >= self.ready_after {
            ProbeVerdict::pass("exists")
        } else {
            ProbeVerdict::fail("not found")
        })
    }
}

async fn run(on: bool, replies: &[&str], ready_after: usize) -> (Trace, Vec<String>) {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let model = Counting::new(replies);
    let (approver, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approver, &stop);
    e.config = Config {
        task_checks: on,
        ..Config::default()
    };
    let disk = Disk {
        checks: AtomicUsize::new(0),
        ready_after,
    };
    let opts = RunOptions {
        probe: Some(&disk),
        ..Default::default()
    };
    let trace = e.run_with(TASK, opts).await;
    (trace, model.seen().into_iter().map(|s| s.text).collect())
}

/// Guard: remove the seeding in `Engine::run_with` and the early `done`
/// is accepted, failing this test.
#[tokio::test]
async fn an_early_done_is_refused_until_the_file_exists() {
    let (trace, turns) = run(true, &[DONE, CLICK, DONE], 1).await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(trace.gate.len(), 1, "{:?}", trace.gate);
    assert_eq!(trace.gate[0].kind, "refused");
    assert!(turns[1].contains("not done"), "{}", turns[1]);
    assert!(turns[1].contains("~/kai-bench/price.txt"), "{}", turns[1]);
    let out = &trace.outcomes[0];
    assert_eq!(out.id, "output1");
    assert_eq!(out.status, ledger::Status::Verified);
}

#[tokio::test]
async fn off_accepts_done_as_before() {
    let (trace, turns) = run(false, &[DONE], usize::MAX).await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(turns.len(), 1);
    assert!(trace.gate.is_empty() && trace.outcomes.is_empty());
}

#[tokio::test]
async fn a_file_that_never_appears_ends_with_asking_not_success() {
    let (trace, _) = run(true, &[DONE, DONE, DONE, DONE], usize::MAX).await;
    assert_ne!(trace.outcome, Some(Outcome::Success));
    assert_eq!(trace.gate.iter().filter(|g| g.kind == "refused").count(), 3);
}
