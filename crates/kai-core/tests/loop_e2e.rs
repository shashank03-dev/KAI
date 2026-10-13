//! The loop driven end to end by `MockClient` against a scripted fake screen.

use async_trait::async_trait;
use kai_core::engine::{
    Actuator, ApprovalChoice, Approver, Engine, Interrupt, NullSink, Observation, Perceptor,
};
use kai_core::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// A fake screen whose hash changes after every action, with a Save button and
/// a password field.
struct FakeScreen {
    frame: AtomicUsize,
}

#[async_trait]
impl Perceptor for FakeScreen {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let f = self.frame.load(Ordering::SeqCst);
        Ok(Observation {
            images: Images {
                screenshot_jpeg: vec![0xff, 0xd8],
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(vec![
                Element {
                    mark: 1,
                    role: Role::Button,
                    name: "Save".into(),
                    bounds: Rect::new(10.0, 10.0, 80.0, 30.0),
                    enabled: true,
                    secure: false,
                    focused: false,
                    value: None,
                },
                Element {
                    mark: 2,
                    role: Role::PasswordField,
                    name: "Password".into(),
                    bounds: Rect::new(10.0, 60.0, 200.0, 30.0),
                    enabled: true,
                    secure: true,
                    focused: false,
                    value: None,
                },
            ]),
            screen_hash: f as u64,
            timing: Default::default(),
            screen: None,
            window_title: None,
            url: None,
        })
    }
}

#[derive(Default)]
struct RecordingHands {
    log: Mutex<Vec<String>>,
}

#[async_trait]
impl Actuator for RecordingHands {
    async fn act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        let at = d
            .target
            .and_then(|t| g.element(t))
            .map(|e| e.bounds.center());
        self.log
            .lock()
            .unwrap()
            .push(format!("{:?} {:?}", d.action, at));
        Ok(())
    }
    async fn open_app(&self, app: &str) -> anyhow::Result<()> {
        self.log.lock().unwrap().push(format!("open_app {app}"));
        Ok(())
    }
    async fn open_url(&self, url: &str) -> anyhow::Result<()> {
        self.log.lock().unwrap().push(format!("open_url {url}"));
        Ok(())
    }
    async fn shell(&self, cmd: &str) -> anyhow::Result<String> {
        self.log.lock().unwrap().push(format!("shell {cmd}"));
        Ok("ok".into())
    }
}

struct Policy(ApprovalChoice, AtomicUsize);
#[async_trait]
impl Approver for Policy {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0
    }
}

struct Flag(AtomicBool);
impl Interrupt for Flag {
    fn stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Advances the fake screen on every act, so progress is visible.
struct Advancing<'a>(&'a FakeScreen, &'a RecordingHands);
#[async_trait]
impl Actuator for Advancing<'_> {
    async fn act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        self.0.frame.fetch_add(1, Ordering::SeqCst);
        self.1.act(d, g).await
    }
    async fn open_app(&self, a: &str) -> anyhow::Result<()> {
        self.0.frame.fetch_add(1, Ordering::SeqCst);
        self.1.open_app(a).await
    }
    async fn open_url(&self, u: &str) -> anyhow::Result<()> {
        self.0.frame.fetch_add(1, Ordering::SeqCst);
        self.1.open_url(u).await
    }
    async fn shell(&self, c: &str) -> anyhow::Result<String> {
        self.0.frame.fetch_add(1, Ordering::SeqCst);
        self.1.shell(c).await
    }
}

fn engine<'a>(
    client: &'a dyn ModelClient,
    screen: &'a FakeScreen,
    hands: &'a dyn Actuator,
    approver: &'a dyn Approver,
    stop: &'a dyn Interrupt,
) -> Engine<'a> {
    Engine {
        client,
        careful: None,
        perceptor: screen,
        actuator: hands,
        approver,
        interrupt: stop,
        sink: &NullSink,
        steering: &kai_core::engine::NoSteering,
        verifier: None,
        human: None,
        skills: &[],
        config: Config::default(),
    }
}

#[tokio::test]
async fn completes_a_task_and_clicks_element_centre() {
    let client = MockClient::new([
        r#"{"thought":"open site","action":"open_url","text":"https://example.com","risk":"none","expect":"site"}"#,
        r#"{"thought":"save","action":"click","target":1,"risk":"low","expect":"saved"}"#,
        r#"{"thought":"finished","action":"done","risk":"none"}"#,
    ]);
    let screen = FakeScreen {
        frame: AtomicUsize::new(0),
    };
    let rec = RecordingHands::default();
    let hands = Advancing(&screen, &rec);
    let approver = Policy(ApprovalChoice::Approve, AtomicUsize::new(0));
    let stop = Flag(AtomicBool::new(false));
    let trace = engine(&client, &screen, &hands, &approver, &stop)
        .run("save the doc")
        .await;

    assert_eq!(trace.outcome, Some(Outcome::Success));
    let log = rec.log.lock().unwrap().clone();
    assert_eq!(log[0], "open_url https://example.com");
    assert_eq!(log[1], "Click Some((50.0, 25.0))");
    assert_eq!(
        approver.1.load(Ordering::SeqCst),
        0,
        "low risk should not ask"
    );
}

#[tokio::test]
async fn password_field_always_asks_even_if_model_says_none() {
    let client = MockClient::new([
        r#"{"thought":"type pw","action":"type","target":2,"text":"hunter2","risk":"none"}"#,
        r#"{"thought":"done","action":"done","risk":"none"}"#,
    ]);
    let screen = FakeScreen {
        frame: AtomicUsize::new(0),
    };
    let rec = RecordingHands::default();
    let hands = Advancing(&screen, &rec);
    let approver = Policy(ApprovalChoice::Approve, AtomicUsize::new(0));
    let stop = Flag(AtomicBool::new(false));
    let mut e = engine(&client, &screen, &hands, &approver, &stop);
    e.config.approval = ApprovalLevel::Autopilot;
    let trace = e.run("log in").await;
    assert_eq!(approver.1.load(Ordering::SeqCst), 1);
    assert_eq!(trace.outcome, Some(Outcome::Success));
}

#[tokio::test]
async fn kill_switch_stops_within_one_step() {
    let client = MockClient::always(r#"{"thought":"wait","action":"wait","risk":"none"}"#);
    let screen = FakeScreen {
        frame: AtomicUsize::new(0),
    };
    let rec = RecordingHands::default();
    let hands = Advancing(&screen, &rec);
    let approver = Policy(ApprovalChoice::Approve, AtomicUsize::new(0));
    let stop = Flag(AtomicBool::new(true));
    let trace = engine(&client, &screen, &hands, &approver, &stop)
        .run("anything")
        .await;
    assert_eq!(trace.outcome, Some(Outcome::Stopped));
    assert!(
        rec.log.lock().unwrap().is_empty(),
        "nothing may act after stop"
    );
}

#[tokio::test]
async fn approval_stop_ends_run() {
    let client =
        MockClient::always(r#"{"thought":"delete","action":"click","target":1,"risk":"high"}"#);
    let screen = FakeScreen {
        frame: AtomicUsize::new(0),
    };
    let rec = RecordingHands::default();
    let hands = Advancing(&screen, &rec);
    let approver = Policy(ApprovalChoice::Stop, AtomicUsize::new(0));
    let stop = Flag(AtomicBool::new(false));
    let trace = engine(&client, &screen, &hands, &approver, &stop)
        .run("x")
        .await;
    assert_eq!(trace.outcome, Some(Outcome::Stopped));
    assert!(rec.log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn no_progress_stops_and_asks() {
    // Screen never changes (plain hands don't advance the frame).
    let client =
        MockClient::always(r#"{"thought":"click","action":"click","target":1,"risk":"none"}"#);
    let screen = FakeScreen {
        frame: AtomicUsize::new(0),
    };
    let hands = RecordingHands::default();
    let approver = Policy(ApprovalChoice::Approve, AtomicUsize::new(0));
    let stop = Flag(AtomicBool::new(false));
    let trace = engine(&client, &screen, &hands, &approver, &stop)
        .run("x")
        .await;
    assert_eq!(trace.outcome, Some(Outcome::AskedUser));
    assert!(trace.steps.len() <= 3);
}

#[tokio::test]
async fn malformed_reply_gets_one_repair() {
    let client = MockClient::new([
        "garbage, not json",
        r#"{"thought":"ok","action":"done","risk":"none"}"#,
    ]);
    let screen = FakeScreen {
        frame: AtomicUsize::new(0),
    };
    let hands = RecordingHands::default();
    let approver = Policy(ApprovalChoice::Approve, AtomicUsize::new(0));
    let stop = Flag(AtomicBool::new(false));
    let trace = engine(&client, &screen, &hands, &approver, &stop)
        .run("x")
        .await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
}

#[tokio::test]
async fn step_limit_is_enforced() {
    let client = MockClient::always(r#"{"thought":"wait","action":"wait","risk":"none"}"#);
    let screen = FakeScreen {
        frame: AtomicUsize::new(0),
    };
    let rec = RecordingHands::default();
    let hands = Advancing(&screen, &rec);
    let approver = Policy(ApprovalChoice::Approve, AtomicUsize::new(0));
    let stop = Flag(AtomicBool::new(false));
    let mut e = engine(&client, &screen, &hands, &approver, &stop);
    e.config.max_steps = 5;
    let trace = e.run("x").await;
    assert_eq!(trace.outcome, Some(Outcome::StepLimit));
    assert_eq!(trace.steps.len(), 5);
}

#[tokio::test]
async fn skill_steps_run_without_the_model() {
    let skill = kai_core::skill::load_from_str(
        "---\nname: open-and-search-web\ndescription: Open a browser and search the web for a query.\nmetadata:\n  kai: '[{\"do\":\"open_url\",\"url\":\"https://duckduckgo.com\"},{\"do\":\"shortcut\",\"keys\":[\"Control\",\"L\"]}]'\n---\nbody\n",
        std::path::Path::new("/x/open-and-search-web"),
    )
    .unwrap();
    // Only one model reply exists: the skill steps must not consume any.
    let client = MockClient::new([r#"{"thought":"done","action":"done","risk":"none"}"#]);
    let screen = FakeScreen {
        frame: AtomicUsize::new(0),
    };
    let rec = RecordingHands::default();
    let hands = Advancing(&screen, &rec);
    let approver = Policy(ApprovalChoice::Approve, AtomicUsize::new(0));
    let stop = Flag(AtomicBool::new(false));
    let skills = [skill];
    let mut e = engine(&client, &screen, &hands, &approver, &stop);
    e.skills = &skills;
    let trace = e.run("search the web for gemma 4").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    let log = rec.log.lock().unwrap().clone();
    assert_eq!(log[0], "open_url https://duckduckgo.com");
    assert_eq!(log[1], "Key None");
    assert_eq!(trace.steps[1].decision.keys, vec!["ctrl", "l"]);
    assert_eq!(trace.steps[0].backend, "skill (no model call)");
}
