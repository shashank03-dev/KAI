mod common;
use async_trait::async_trait;
use common::{Counting, Screen, approve_all, engine, flag};
use kai_core::computer::*;
use kai_core::*;
use std::sync::Mutex;

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
        panic!("canonical action used legacy driver")
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
const MOVE: &str = r#"{"thought":"hover File","action":"computer","computer":{"type":"move","target":{"kind":"element","observation":1,"mark":1}},"risk":"none"}"#;
const DONE: &str = r#"{"thought":"finished","action":"done","risk":"none"}"#;

#[tokio::test]
async fn opt_in_model_receives_schema_and_uses_shared_move_executor() {
    let model = Counting::new(&[MOVE, DONE]);
    let screen = Screen::marks();
    let hands = Hands::default();
    let stop = flag();
    let approve = approve_all();
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.config.computer_tools_v1 = true;
    let trace = e.run("Hover File").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(
        hands.0.lock().unwrap().as_slice(),
        &[ComputerAction::Move {
            target: Target::Element {
                observation: 1,
                mark: 1
            },
            modifiers: vec![],
        }]
    );
    let seen = model.seen();
    assert!(seen[0].schema["properties"]["computer"].is_object());
    assert!(seen[0].text.contains("observation_id: 1"));
    assert_eq!(trace.steps[0].path.as_deref(), Some("computer"));
}

#[tokio::test]
async fn legacy_model_cannot_enable_computer_tools_in_its_reply() {
    let model = Counting::new(&[MOVE, MOVE, DONE]);
    let screen = Screen::marks();
    let hands = Hands::default();
    let stop = flag();
    let approve = approve_all();
    let trace = engine(&model, &screen, &hands, &approve, &stop)
        .run("Hover File")
        .await;
    assert!(hands.0.lock().unwrap().is_empty());
    assert!(!model.seen()[0].schema["properties"]["computer"].is_object());
    assert!(
        trace
            .steps
            .iter()
            .all(|s| s.path.as_deref() != Some("computer"))
    );
}

#[tokio::test]
async fn a_model_cannot_invent_an_element_reference() {
    let bad = MOVE.replace("\"mark\":1", "\"mark\":999");
    let model = Counting::new(&[&bad]);
    let screen = Screen::marks();
    let hands = Hands::default();
    let stop = flag();
    let approve = approve_all();
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.config.computer_tools_v1 = true;
    let trace = e.run("Hover a missing element").await;
    assert_eq!(trace.outcome, Some(Outcome::AskedUser));
    assert!(hands.0.lock().unwrap().is_empty());
}
