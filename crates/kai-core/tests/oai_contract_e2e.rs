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
