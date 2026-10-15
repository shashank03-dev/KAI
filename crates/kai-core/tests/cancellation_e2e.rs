mod common;

use async_trait::async_trait;
use common::{Flag, Hands, Screen, approve_all, engine, flag};
use kai_core::client::{ClientError, ModelReply, ModelRequest};
use kai_core::*;
use std::sync::atomic::Ordering;
use std::time::Duration;

struct StopOnReply<'a>(&'a Flag, MockClient);

#[async_trait]
impl ModelClient for StopOnReply<'_> {
    fn backend(&self) -> Backend { Backend::Mock }
    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        self.0.0.store(true, Ordering::SeqCst);
        self.1.decide(req).await
    }
}

#[tokio::test]
async fn stop_during_decision_prevents_the_returned_click() {
    let stop = flag();
    let client = StopOnReply(&stop, MockClient::new([
        r#"{"thought":"click File","action":"click","target":1,"risk":"none"}"#,
    ]));
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let approve = approve_all();
    let trace = engine(&client, &screen, &hands, &approve, &stop).run("Click File").await;
    assert!(hands.log.lock().unwrap().is_empty());
    assert_eq!(trace.outcome, Some(Outcome::Stopped));
}

struct StopOnApproval<'a>(&'a Flag);
#[async_trait]
impl Approver for StopOnApproval<'_> {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        self.0.0.store(true, Ordering::SeqCst);
        ApprovalChoice::Approve
    }
}

#[tokio::test]
async fn stop_during_approval_prevents_the_approved_command() {
    let stop = flag();
    let client = MockClient::new([
        r#"{"thought":"run","action":"shell","text":"echo hello","risk":"high"}"#,
    ]);
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let trace = engine(&client, &screen, &hands, &StopOnApproval(&stop), &stop)
        .run("Run a command").await;
    assert!(hands.log.lock().unwrap().is_empty());
    assert_eq!(trace.outcome, Some(Outcome::Stopped));
}

struct Hanging;
#[async_trait]
impl ModelClient for Hanging {
    fn backend(&self) -> Backend { Backend::Mock }
    async fn decide(&self, _: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn cancellation_does_not_wait_for_a_hanging_model() {
    let stop = flag();
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let approve = approve_all();
    let runner = engine(&Hanging, &screen, &hands, &approve, &stop);
    let trip = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        stop.0.store(true, Ordering::SeqCst);
    };
    let (trace, ()) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(1), runner.run("Click File")), trip
    );
    assert_eq!(trace.expect("stop must interrupt inference").outcome, Some(Outcome::Stopped));
    assert!(hands.log.lock().unwrap().is_empty());
}
