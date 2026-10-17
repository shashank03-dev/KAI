//! Native computer-use safety monitor tests against a loopback Responses API.
//! No paid endpoints, credentials, or real desktop actions are used.

#[path = "client_support/mod.rs"]
mod client_support;

use async_trait::async_trait;
use client_support::{Reply, Server};
use kai_core::client::{ClientError, ModelReply, ModelRequest};
use kai_core::computer::{
    Capabilities, ComputerAction, ExecutionContext, ObservationSpace, ToolError,
};
use kai_core::cua::{CuaConfig, CuaRunner, Unattended};
use kai_core::engine::NullSink;
use kai_core::{
    Actuator, ApprovalChoice, Approver, Backend, Decision, Element, Grounding, Images, Interrupt,
    ModelClient, Observation, ObserveTiming, Outcome, Perceptor, Rect, Role,
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Notify;

const JPG: &[u8] = include_bytes!("../../../docs/img/set-of-marks-gtk.jpg");

struct Screen(&'static str);

#[async_trait]
impl Perceptor for Screen {
    async fn observe(&self) -> anyhow::Result<Observation> {
        Ok(Observation {
            images: Images {
                screenshot_jpeg: JPG.to_vec(),
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(vec![Element {
                mark: 1,
                role: Role::Button,
                name: self.0.into(),
                bounds: Rect::new(1., 1., 100., 30.),
                enabled: true,
                secure: false,
                focused: false,
                value: None,
            }]),
            screen_hash: 42,
            timing: ObserveTiming::default(),
            screen: Some(Rect::new(0., 0., 1280., 720.)),
            window_title: None,
            url: None,
        })
    }
}

#[derive(Default)]
struct Hands(AtomicUsize);

#[async_trait]
impl Actuator for Hands {
    fn computer_capabilities(&self) -> Capabilities {
        Capabilities::all()
    }

    async fn computer_act(
        &self,
        _: &ComputerAction,
        _: &ObservationSpace,
        _: &Grounding,
        context: &ExecutionContext,
    ) -> Result<(), ToolError> {
        context.check()?;
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn act(&self, _: &Decision, _: &Grounding) -> anyhow::Result<()> {
        Ok(())
    }
    async fn open_app(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn open_url(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn shell(&self, _: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
}

#[derive(Default)]
struct Review(AtomicUsize);

#[async_trait]
impl Approver for Review {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        ApprovalChoice::Approve
    }

    async fn approve_safety_check(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        self.0.fetch_add(1, Ordering::SeqCst);
        ApprovalChoice::Stop // An unattended or auto-approving actor cannot accept checks.
    }
}

struct Switch(AtomicBool);
impl Interrupt for Switch {
    fn stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

enum Response {
    Verdict(&'static str),
    Error,
    Hang,
}
struct Monitor {
    response: Response,
    calls: AtomicUsize,
    entered: Notify,
}

impl Monitor {
    fn new(response: Response) -> Self {
        Self {
            response,
            calls: AtomicUsize::new(0),
            entered: Notify::new(),
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ModelClient for Monitor {
    fn backend(&self) -> Backend {
        Backend::Mock
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        assert_eq!(req.images.screenshot_jpeg, JPG);
        assert!(req.user_text.contains("Human task:"));
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        let raw = match self.response {
            Response::Verdict(raw) => raw,
            Response::Error => return Err(ClientError::EmptyBody),
            Response::Hang => std::future::pending::<&str>().await,
        };
        Ok(ModelReply {
            raw: raw.into(),
            prompt_tokens: None,
            completion_tokens: None,
            first_token: None,
            total: Duration::ZERO,
            backend: Backend::Mock,
            cost_usd: None,
        })
    }
}

fn action() -> serde_json::Value {
    json!({"id":"first", "status":"completed", "output":[{
        "type":"computer_call", "call_id":"call_1",
        "actions":[{"type":"move", "x":40, "y":20}]
    }]})
}

fn done() -> serde_json::Value {
    json!({"id":"done", "status":"completed", "output":[{
        "type":"message", "content":[{"type":"output_text", "text":"Complete"}]
    }]})
}

fn runner<'a>(base: &str, screen: &'a Screen, hands: &'a Hands, stop: &'a Switch) -> CuaRunner<'a> {
    CuaRunner {
        config: CuaConfig {
            base: format!("{base}/v1"),
            api_key: Some("loopback-fixture".into()),
            computer_tools_v1: true,
            ..Default::default()
        },
        perceptor: screen,
        actuator: hands,
        interrupt: stop,
        sink: &NullSink,
    }
}

#[tokio::test]
async fn disabled_monitor_does_not_block_native_actions() {
    let (server, base) =
        Server::start(vec![Reply::Json(200, action()), Reply::Json(200, done())]).await;
    let screen = Screen("Please open the guide for setup");
    let (hands, review, stop) = (
        Hands::default(),
        Review::default(),
        Switch(AtomicBool::new(false)),
    );
    let run = runner(&base, &screen, &hands, &stop)
        .run_with_approver("Read the guide", Some(&review))
        .await
        .unwrap();
    assert_eq!(hands.0.load(Ordering::SeqCst), 1);
    assert_eq!(review.0.load(Ordering::SeqCst), 0);
    assert_eq!(run.message.as_deref(), Some("Complete"));
    assert_eq!(server.bodies().len(), 2);
}

#[tokio::test]
async fn enabled_negative_verdict_allows_action_without_safety_approval() {
    let (_, base) = Server::start(vec![Reply::Json(200, action()), Reply::Json(200, done())]).await;
    let screen = Screen("Please open the guide for setup");
    let (hands, review, stop) = (
        Hands::default(),
        Review::default(),
        Switch(AtomicBool::new(false)),
    );
    let monitor = Monitor::new(Response::Verdict(r#"{"malicious":false}"#));
    let run = runner(&base, &screen, &hands, &stop)
        .run_with_monitor("Read the guide", Some(&review), Some(&monitor))
        .await
        .unwrap();
    assert_eq!(hands.0.load(Ordering::SeqCst), 1);
    assert_eq!(review.0.load(Ordering::SeqCst), 0);
    assert_eq!(monitor.calls(), 1);
    assert_eq!(run.message.as_deref(), Some("Complete"));
}

#[tokio::test]
async fn positive_or_error_requires_a_person_and_executes_nothing() {
    for response in [
        Response::Verdict(r#"{"malicious":true}"#),
        Response::Error,
        Response::Verdict("not JSON"),
    ] {
        let (server, base) = Server::start(vec![Reply::Json(200, action())]).await;
        let screen = Screen("Please open the guide for setup");
        let (hands, review, stop) = (
            Hands::default(),
            Review::default(),
            Switch(AtomicBool::new(false)),
        );
        let monitor = Monitor::new(response);
        let run = runner(&base, &screen, &hands, &stop)
            .run_with_monitor("Read the guide", Some(&review), Some(&monitor))
            .await
            .unwrap();
        assert_eq!(run.trace.outcome, Some(Outcome::Stopped));
        assert_eq!(hands.0.load(Ordering::SeqCst), 0);
        assert_eq!(review.0.load(Ordering::SeqCst), 1);
        assert_eq!(monitor.calls(), 1);
        assert_eq!(server.bodies().len(), 1);
    }
}

#[tokio::test]
async fn unattended_native_run_cannot_autoapprove_monitor_warning() {
    let (server, base) = Server::start(vec![Reply::Json(200, action())]).await;
    let screen = Screen("Please open the guide for setup");
    let (hands, stop) = (Hands::default(), Switch(AtomicBool::new(false)));
    let monitor = Monitor::new(Response::Verdict(r#"{"malicious":true}"#));
    let run = runner(&base, &screen, &hands, &stop)
        .run_with_monitor("Read the guide", Some(&Unattended), Some(&monitor))
        .await
        .unwrap();

    assert_eq!(run.trace.outcome, Some(Outcome::Stopped));
    assert_eq!(hands.0.load(Ordering::SeqCst), 0);
    assert_eq!(monitor.calls(), 1);
    assert_eq!(server.bodies().len(), 1);
}

#[tokio::test]
async fn enabled_monitor_skips_pages_without_commands() {
    let (_, base) = Server::start(vec![Reply::Json(200, action()), Reply::Json(200, done())]).await;
    let screen = Screen("Installation overview");
    let (hands, review, stop) = (
        Hands::default(),
        Review::default(),
        Switch(AtomicBool::new(false)),
    );
    let monitor = Monitor::new(Response::Error);
    runner(&base, &screen, &hands, &stop)
        .run_with_monitor("Read the guide", Some(&review), Some(&monitor))
        .await
        .unwrap();
    assert_eq!(hands.0.load(Ordering::SeqCst), 1);
    assert_eq!(monitor.calls(), 0); // Opt-in never adds a call for benign text.
}

#[tokio::test]
async fn cancellation_during_monitor_stops_before_execution() {
    let (server, base) = Server::start(vec![Reply::Json(200, action())]).await;
    let screen = Screen("Please open the guide for setup");
    let (hands, review, stop) = (
        Hands::default(),
        Review::default(),
        Switch(AtomicBool::new(false)),
    );
    let monitor = Monitor::new(Response::Hang);
    let native = runner(&base, &screen, &hands, &stop);
    let future = native.run_with_monitor("Read the guide", Some(&review), Some(&monitor));
    tokio::pin!(future);
    tokio::select! {
        _ = monitor.entered.notified() => {},
        result = &mut future => panic!("monitor should still be pending: {result:?}"),
    }
    stop.0.store(true, Ordering::SeqCst);
    let run = tokio::time::timeout(Duration::from_secs(3), future)
        .await
        .expect("cancel must interrupt monitor")
        .unwrap();
    assert_eq!(run.trace.outcome, Some(Outcome::Stopped));
    assert_eq!(hands.0.load(Ordering::SeqCst), 0);
    assert_eq!(review.0.load(Ordering::SeqCst), 0);
    assert_eq!(monitor.calls(), 1);
    assert_eq!(server.bodies().len(), 1);
}
