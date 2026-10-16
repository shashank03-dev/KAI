//! Open models on OpenAI's computer-tool contract, through the real engine.
mod common;
use async_trait::async_trait;
use common::{Counting, Screen, approve_all, engine, flag};
use kai_core::computer::*;
use kai_core::oai_contract::CoordFrame;
use kai_core::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct Hands(Mutex<Vec<ComputerAction>>);
#[async_trait]
impl Actuator for Hands {
    fn computer_capabilities(&self) -> Capabilities {
        Capabilities::all()
    }
    async fn computer_act(
        &self,
        action: &ComputerAction,
        _: &ObservationSpace,
        _: &Grounding,
        ctx: &ExecutionContext,
    ) -> Result<(), ToolError> {
        ctx.check()?;
        self.0.lock().unwrap().push(action.clone());
        Ok(())
    }
    async fn act(&self, _: &Decision, _: &Grounding) -> anyhow::Result<()> {
        panic!("a contract action used the legacy driver")
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

const CLICK: &str = r#"{"thought":"File menu is closed; open it.","actions":[{"type":"click","button":"left","x":null,"y":null,"mark":1,"keys":null}],"message":null,"phase":"commentary"}"#;
const DONE: &str = r#"{"thought":"Opened.","actions":[],"message":"The File menu is open.","phase":"final_answer"}"#;

#[tokio::test]
async fn an_open_model_drives_kai_in_openais_vocabulary() {
    let model = Counting::new(&[CLICK, DONE]);
    let screen = Screen::marks();
    let hands = Hands::default();
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Marks);
    let trace = e.run("Open the File menu").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(
        hands.0.lock().unwrap().as_slice(),
        &[ComputerAction::Click {
            target: Target::Element {
                observation: 1,
                mark: 1
            },
            button: MouseButton::Left,
            count: 1,
            modifiers: vec![],
        }]
    );
    let seen = model.seen();
    assert!(seen[0].system.contains("for the `computer` tool"));
    assert!(seen[0].system.contains("Give `mark`"));
    assert_eq!(seen[0].schema["properties"]["actions"]["maxItems"], 1);
    assert_eq!(trace.steps[0].path.as_deref(), Some("computer"));
}

/// Counts requests for the unmarked frame.
struct Eyes<'a>(&'a Screen, AtomicUsize);
#[async_trait]
impl Perceptor for Eyes<'_> {
    async fn observe(&self) -> anyhow::Result<Observation> {
        self.0.observe().await
    }
    async fn observe_tree_only(&self) -> anyhow::Result<Observation> {
        self.0.observe_tree_only().await
    }
    async fn native_frame(&self) -> Option<NativeFrame> {
        self.1.fetch_add(1, Ordering::SeqCst);
        None
    }
}

#[tokio::test]
async fn a_step_without_a_screenshot_never_fetches_the_unmarked_frame() {
    // The fake observation's image is not a decodable frame, like a
    // tree-only step or the privacy latch after a secret was typed: the
    // contract must not take its own screenshot behind Kai's back.
    let model = Counting::new(&[CLICK, DONE]);
    let screen = Screen::marks();
    let eyes = Eyes(&screen, AtomicUsize::new(0));
    let hands = Hands::default();
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.perceptor = &eyes;
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Norm1000);
    e.config.max_steps = 2;
    let trace = e.run("Open the File menu").await;
    assert_eq!(eyes.1.load(Ordering::SeqCst), 0);
    assert!(hands.0.lock().unwrap().is_empty());
    assert_ne!(trace.outcome, Some(Outcome::Success));
}

/// The page is on `evil.example.net`, a site the task never names.
struct OffTask<'a>(&'a Screen);
#[async_trait]
impl Perceptor for OffTask<'_> {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let mut o = self.0.observe().await?;
        o.url = Some("https://evil.example.net/login".into());
        Ok(o)
    }
}

/// Approves actions, answers safety checks with `answer`, and counts them.
struct Person(ApprovalChoice, AtomicUsize);
#[async_trait]
impl Approver for Person {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        ApprovalChoice::Approve
    }
    async fn approve_safety_check(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0
    }
}

async fn off_task_run(answer: ApprovalChoice, checks: bool) -> (Trace, usize, usize) {
    let model = Counting::new(&[CLICK, CLICK, DONE]);
    let screen = Screen::marks();
    let eyes = OffTask(&screen);
    let hands = Hands::default();
    let person = Person(answer, AtomicUsize::new(0));
    let stop = flag();
    let mut e = engine(&model, &screen, &hands, &person, &stop);
    e.perceptor = &eyes;
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Marks);
    e.config.local_safety_checks = checks;
    let trace = e.run("Open the File menu on bmsce.ac.in").await;
    let acted = hands.0.lock().unwrap().len();
    (trace, acted, person.1.load(Ordering::SeqCst))
}

#[tokio::test]
async fn an_off_task_site_needs_the_person_before_any_click() {
    let (trace, acted, asked) = off_task_run(ApprovalChoice::Stop, true).await;
    assert_eq!(acted, 0, "clicked an off-task site without the person");
    assert_eq!(asked, 1);
    assert_eq!(trace.outcome, Some(Outcome::Stopped));
    // The guard is what stopped it: without the switch the clicks run.
    let (_, acted, asked) = off_task_run(ApprovalChoice::Stop, false).await;
    assert_eq!((acted, asked), (2, 0));
}

#[tokio::test]
async fn an_accepted_site_is_not_asked_about_again_this_run() {
    let (trace, acted, asked) = off_task_run(ApprovalChoice::Approve, true).await;
    assert_eq!(acted, 2);
    assert_eq!(asked, 1, "asked again about a site the person allowed");
    assert_eq!(trace.outcome, Some(Outcome::Success));
}

const TWO: &str = r#"{"thought":"Open File, then Edit.","actions":[{"type":"click","button":"left","x":null,"y":null,"mark":1,"keys":null},{"type":"click","button":"left","x":null,"y":null,"mark":2,"keys":null}],"message":null,"phase":"commentary"}"#;

#[tokio::test]
async fn a_two_action_turn_costs_one_model_call() {
    let model = Counting::new(&[TWO, DONE]);
    let screen = Screen::marks();
    let hands = Hands::default();
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Marks);
    e.config.oai_batch = 3;
    let trace = e.run("Open File then Edit").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    let marks: Vec<u32> = hands
        .0
        .lock()
        .unwrap()
        .iter()
        .filter_map(|a| match a {
            ComputerAction::Click {
                target: Target::Element { mark, .. },
                ..
            } => Some(*mark),
            _ => None,
        })
        .collect();
    assert_eq!(marks, [1, 2]);
    assert_eq!(model.seen().len(), 2, "the queued click called the model");
    assert!(model.seen()[0].system.contains("at most 3"));
}

/// After the first click, element #1 has a new name: a different layout.
struct Shifting<'a>(&'a Screen, &'a Hands);
#[async_trait]
impl Perceptor for Shifting<'_> {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let mut o = self.0.observe().await?;
        if !self.1.0.lock().unwrap().is_empty()
            && let Grounding::Marks(els) = &mut o.grounding
        {
            els[0].name = "A dialog opened".into();
        }
        Ok(o)
    }
}

#[tokio::test]
async fn a_changed_layout_stops_the_turn_and_the_model_hears_why() {
    let model = Counting::new(&[TWO, DONE]);
    let screen = Screen::marks();
    let hands = Hands::default();
    let eyes = Shifting(&screen, &hands);
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.perceptor = &eyes;
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Marks);
    e.config.oai_batch = 3;
    e.run("Open File then Edit").await;
    assert_eq!(
        hands.0.lock().unwrap().len(),
        1,
        "clicked into a changed screen"
    );
    let next = &model.seen()[1].text;
    assert!(
        next.contains(
            "Harness note: actions 1 to 1 of 2 ran; the rest did not: the screen layout changed."
        ),
        "{next}"
    );
}

const CLICK2: &str = r#"{"thought":"Edit, maybe.","actions":[{"type":"click","button":"left","x":null,"y":null,"mark":2,"keys":null}],"message":null,"phase":"commentary"}"#;

#[tokio::test]
async fn of_three_sampled_replies_the_majority_acts() {
    let model = Counting::new(&[CLICK, CLICK2, CLICK, DONE, DONE, DONE]);
    let screen = Screen::marks();
    let hands = Hands::default();
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Marks);
    e.config.step_candidates = 3;
    let trace = e.run("Open the File menu").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    let acted = hands.0.lock().unwrap().clone();
    assert_eq!(acted.len(), 1, "only the chosen candidate runs");
    assert!(matches!(
        acted[0],
        ComputerAction::Click {
            target: Target::Element { mark: 1, .. },
            ..
        }
    ));
    assert_eq!(model.seen().len(), 6);
}

/// A 1000x500 frame (just the JPEG header Kai reads) over a 2000x1000
/// screen; `grid` when nothing on it is marked.
struct Sized<'a>(&'a Screen);
#[async_trait]
impl Perceptor for Sized<'_> {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let mut o = self.0.observe().await?;
        let mut jpeg = vec![
            0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0xF4, 0x03, 0xE8,
        ];
        jpeg.resize(32, 0);
        o.images.screenshot_jpeg = jpeg;
        o.screen = Some(Rect::new(0., 0., 2000., 1000.));
        Ok(o)
    }
}

const DESCRIBED: &str = r#"{"thought":"Save it.","actions":[{"type":"click","button":"left","x":null,"y":null,"target":"the blue Save icon in the toolbar","keys":null}],"message":null,"phase":"commentary"}"#;

#[tokio::test]
async fn a_described_target_is_found_by_the_grounder() {
    let planner = Counting::new(&[DESCRIBED, DONE]);
    let grounder = Counting::new(&[r#"{"x":500,"y":500}"#]);
    let screen = Screen {
        grid: true,
        ..Screen::marks()
    };
    let eyes = Sized(&screen);
    let hands = Hands::default();
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&planner, &screen, &hands, &approve, &stop);
    e.perceptor = &eyes;
    e.grounder = Some(Grounder {
        client: &grounder,
        frame: CoordFrame::Norm1000,
    });
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Pixels);
    e.config.tree_only = false;
    let trace = e.run("Save the drawing").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    // (500, 500) of 1000 on a 1000x500 image is pixel (500, 250).
    assert_eq!(
        hands.0.lock().unwrap().as_slice(),
        &[ComputerAction::Click {
            target: Target::Point {
                observation: 1,
                x: 500.,
                y: 250.
            },
            button: MouseButton::Left,
            count: 1,
            modifiers: vec![],
        }]
    );
    assert_eq!(grounder.seen()[0].text, "the blue Save icon in the toolbar");
    assert!(
        planner.seen()[0]
            .system
            .contains("Nothing on this screen is numbered")
    );
}

#[tokio::test]
async fn marked_screens_never_call_the_grounder() {
    let planner = Counting::new(&[CLICK, DONE]);
    let grounder = Counting::new(&[r#"{"x":1,"y":1}"#]);
    let screen = Screen::marks();
    let hands = Hands::default();
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&planner, &screen, &hands, &approve, &stop);
    e.grounder = Some(Grounder {
        client: &grounder,
        frame: CoordFrame::Norm1000,
    });
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Marks);
    e.run("Open the File menu").await;
    assert!(grounder.seen().is_empty());
    assert!(
        !planner.seen()[0]
            .system
            .contains("Nothing on this screen is numbered")
    );
}

#[tokio::test]
async fn a_description_on_a_marked_screen_cannot_invoke_the_grounder() {
    let planner = Counting::new(&[DESCRIBED]);
    let grounder = Counting::new(&[r#"{"x":500,"y":500}"#]);
    let screen = Screen::marks();
    let hands = Hands::default();
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&planner, &screen, &hands, &approve, &stop);
    e.grounder = Some(Grounder {
        client: &grounder,
        frame: CoordFrame::Norm1000,
    });
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Marks);
    let trace = e.run("Open the File menu").await;
    assert_ne!(trace.outcome, Some(Outcome::Success));
    assert!(grounder.seen().is_empty());
    assert!(hands.0.lock().unwrap().is_empty());
}

struct StopApprovals;
#[async_trait]
impl Approver for StopApprovals {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        ApprovalChoice::Stop
    }
}

#[tokio::test]
async fn an_unmarked_grounder_click_requires_approval_even_on_autopilot() {
    let planner = Counting::new(&[DESCRIBED]);
    let grounder = Counting::new(&[r#"{"x":500,"y":500}"#]);
    let screen = Screen {
        grid: true,
        ..Screen::marks()
    };
    let eyes = Sized(&screen);
    let hands = Hands::default();
    let stop = flag();
    let approve = StopApprovals;
    let mut e = engine(&planner, &screen, &hands, &approve, &stop);
    e.perceptor = &eyes;
    e.grounder = Some(Grounder {
        client: &grounder,
        frame: CoordFrame::Norm1000,
    });
    e.config.computer_tools_v1 = true;
    e.config.oai_contract = Some(CoordFrame::Pixels);
    e.config.approval = risk::ApprovalLevel::Autopilot;
    e.config.tree_only = false;
    let trace = e.run("Save the drawing").await;
    assert_eq!(trace.outcome, Some(Outcome::Stopped), "{trace:?}");
    assert!(hands.0.lock().unwrap().is_empty());
}
