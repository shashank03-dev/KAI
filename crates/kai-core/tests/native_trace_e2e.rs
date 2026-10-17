//! Recording a native model's actual request, including its unmarked image.
mod common;

use async_trait::async_trait;
use common::{Counting, Hands, Screen, approve_all, engine, flag};
use kai_core::client::{ClientError, ModelClient, ModelReply, ModelRequest};
use kai_core::computer::{
    Capabilities, ComputerAction, ExecutionContext, ObservationSpace, ToolError,
};
use kai_core::native_models::NativeModel;
use kai_core::oai_contract::CoordFrame;
use kai_core::*;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

const JPEG: &[u8] = include_bytes!("../../../docs/img/set-of-marks-gtk.jpg");
const FARA_DONE: &str = r#"<tool_call>{"name":"computer_use","arguments":{"action":"terminate","answer":"complete"}}</tool_call>"#;
const FARA_CLICK: &str = r#"<tool_call>{"name":"computer_use","arguments":{"action":"left_click","coordinate":[80,90]}}</tool_call>"#;

struct Eyes;

fn unmarked_jpeg() -> Vec<u8> {
    // A valid JPEG COM segment before EOI makes the unmarked image visibly
    // distinguishable (by bytes) from the observation's annotated screenshot.
    let mut jpeg = JPEG.to_vec();
    assert!(jpeg.ends_with(&[0xff, 0xd9]));
    jpeg.splice(
        jpeg.len() - 2..jpeg.len() - 2,
        [0xff, 0xfe, 0, 4, 0x4b, 0x41],
    );
    jpeg
}

#[async_trait]
impl Perceptor for Eyes {
    async fn observe(&self) -> anyhow::Result<Observation> {
        Ok(Observation {
            images: Images {
                screenshot_jpeg: JPEG.to_vec(),
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(vec![]),
            screen_hash: 1,
            timing: Default::default(),
            screen: None,
            window_title: None,
            url: None,
        })
    }

    async fn native_frame(&self) -> Option<NativeFrame> {
        Some(NativeFrame {
            image: unmarked_jpeg(),
            mime: "image/jpeg",
            width: 1280,
            height: 720,
        })
    }
}

struct CapturingModel {
    model: Counting,
    sent_image: Mutex<Vec<u8>>,
}

#[async_trait]
impl ModelClient for CapturingModel {
    fn backend(&self) -> Backend {
        self.model.backend()
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        *self.sent_image.lock().unwrap() = req.images.screenshot_jpeg.clone();
        self.model.decide(req).await
    }
}

struct ChangingEyes<'a>(&'a Screen);

#[async_trait]
impl Perceptor for ChangingEyes<'_> {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let mut obs = Eyes.observe().await?;
        let (w, h) = kai_core::cua::jpeg_size(&obs.images.screenshot_jpeg).unwrap();
        obs.screen = Some(Rect::new(0.0, 0.0, w as f32, h as f32));
        Ok(obs)
    }

    async fn native_frame(&self) -> Option<NativeFrame> {
        let index = self.0.frame.load(Ordering::SeqCst);
        let mut image = unmarked_jpeg();
        let last_comment_byte = image.len() - 3;
        image[last_comment_byte] = b'0' + index as u8;
        Some(NativeFrame {
            image,
            mime: "image/jpeg",
            width: if index == 0 { 1280 } else { 1440 },
            height: if index == 0 { 800 } else { 900 },
        })
    }
}

struct NativeHands<'a>(&'a Screen);

#[async_trait]
impl Actuator for NativeHands<'_> {
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
        self.0.frame.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn act(&self, _: &Decision, _: &Grounding) -> anyhow::Result<()> {
        anyhow::bail!("not a legacy action")
    }
    async fn open_app(&self, _: &str) -> anyhow::Result<()> {
        anyhow::bail!("not an app action")
    }
    async fn open_url(&self, _: &str) -> anyhow::Result<()> {
        anyhow::bail!("not a URL action")
    }
    async fn shell(&self, _: &str) -> anyhow::Result<String> {
        anyhow::bail!("not a shell action")
    }
}

struct CapturingSequence {
    model: Counting,
    sent_images: Mutex<Vec<Vec<u8>>>,
}

/// Scripted Holo API calls: plain text decisions fail, forcing the engine to
/// use the native function transport before entering the shared executor.
struct HoloToolModel {
    model: Counting,
}

#[async_trait]
impl ModelClient for HoloToolModel {
    fn backend(&self) -> Backend {
        self.model.backend()
    }

    async fn decide(&self, _: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        Err(ClientError::Incomplete(
            "plain text path forbidden for Holo".into(),
        ))
    }

    async fn decide_with_tool(
        &self,
        req: &ModelRequest<'_>,
        tool: &serde_json::Value,
    ) -> Result<ModelReply, ClientError> {
        assert_eq!(tool["function"]["name"], "computer_use");
        assert!(req.schema.is_null());
        assert!(!req.images.screenshot_jpeg.is_empty());
        self.model.decide(req).await
    }
}

#[tokio::test]
async fn holo_tool_calls_execute_through_the_shared_native_engine() {
    let model = HoloToolModel {
        model: Counting::new(&[
            r#"{"action":"left_click","coordinate":[500,500]}"#,
            r#"{"action":"terminate","answer":"verified"}"#,
        ]),
    };
    let screen = Screen::marks();
    let changing = ChangingEyes(&screen);
    let hands = NativeHands(&screen);
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.perceptor = &changing;
    e.config.native_model = Some(NativeModel::Holo31);
    e.config.computer_tools_v1 = true;
    let root = std::env::temp_dir().join(format!(
        "kai-holo-tool-trace-{}-{}",
        std::process::id(),
        jiff::Timestamp::now().as_nanosecond()
    ));
    let recorder = TraceRecorder::new(
        &root,
        RunMeta::new("holo-tool", "click then finish", "test", "mock", "holo31"),
    )
    .unwrap();
    let trace = e
        .run_with(
            "click then finish",
            RunOptions {
                recorder: Some(&recorder),
                ..RunOptions::default()
            },
        )
        .await;
    assert_eq!(trace.outcome, Some(Outcome::Success), "{:?}", trace.errors);
    assert_eq!(screen.frame.load(Ordering::SeqCst), 1, "{trace:#?}");
    assert_eq!(trace.steps[0].path.as_deref(), Some("computer"));
    assert_eq!(model.model.count(), 2);
    assert!(model.model.seen()[0].system.contains("Holo3.1"));
    let spec = NativeModel::Holo31.tool_spec().unwrap();
    assert_eq!(trace.steps[0].call.tool_spec.as_ref(), Some(&spec));
    assert_eq!(trace.steps[1].call.tool_spec.as_ref(), Some(&spec));
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("trace.json")).unwrap()).unwrap();
    assert_eq!(saved["steps"][0]["tool_spec"], spec);
    std::fs::remove_dir_all(root).unwrap();
}

#[async_trait]
impl ModelClient for CapturingSequence {
    fn backend(&self) -> Backend {
        self.model.backend()
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        self.sent_images
            .lock()
            .unwrap()
            .push(req.images.screenshot_jpeg.clone());
        self.model.decide(req).await
    }
}

#[tokio::test]
async fn native_multi_step_trace_records_each_actual_frame_and_matching_system() {
    let model = CapturingSequence {
        model: Counting::new(&[FARA_CLICK, FARA_DONE]),
        sent_images: Mutex::default(),
    };
    let screen = Screen::marks();
    let changing = ChangingEyes(&screen);
    let hands = NativeHands(&screen);
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.perceptor = &changing;
    e.config.native_model = Some(NativeModel::Fara15);
    e.config.computer_tools_v1 = true;
    let root = std::env::temp_dir().join(format!(
        "kai-native-multi-trace-{}-{}",
        std::process::id(),
        jiff::Timestamp::now().as_nanosecond()
    ));
    let recorder = TraceRecorder::new(
        &root,
        RunMeta::new("native-resize", "click and finish", "test", "mock", "fara"),
    )
    .unwrap();

    let trace = e
        .run_with(
            "click and finish",
            RunOptions {
                recorder: Some(&recorder),
                ..RunOptions::default()
            },
        )
        .await;
    assert_eq!(trace.outcome, Some(Outcome::Success), "{:?}", trace.errors);
    let seen = model.model.seen();
    assert_eq!(seen.len(), 2);
    assert_ne!(seen[0].system, seen[1].system);
    assert!(seen[0].system.contains("1280 x 800"));
    assert!(seen[1].system.contains("1440 x 900"));
    let sent = model.sent_images.lock().unwrap();
    for (index, request) in seen.iter().enumerate() {
        let hash = kai_core::recorder::fnv1a_hex(&request.system);
        assert_eq!(
            trace.steps[index].call.prompt_system_hash.as_deref(),
            Some(hash.as_str())
        );
        assert_eq!(
            std::fs::read_to_string(root.join(format!("systems/{hash}.txt"))).unwrap(),
            request.system
        );
        assert_eq!(
            std::fs::read(root.join(format!("frames/step-{index:03}.jpg"))).unwrap(),
            sent[index]
        );
        assert_ne!(sent[index], JPEG);
    }
    assert_ne!(sent[0], sent[1]);
    assert_eq!(
        std::fs::read_to_string(root.join("system.txt")).unwrap(),
        seen[0].system
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn recorded_native_step_matches_actual_model_prompt_and_image() {
    let model = CapturingModel {
        model: Counting::new(&[FARA_DONE]),
        sent_image: Mutex::default(),
    };
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.perceptor = &Eyes;
    e.config.native_model = Some(NativeModel::Fara15);
    e.config.computer_tools_v1 = true;
    let root = std::env::temp_dir().join(format!(
        "kai-native-trace-{}-{}",
        std::process::id(),
        jiff::Timestamp::now().as_nanosecond()
    ));
    let recorder = TraceRecorder::new(
        &root,
        RunMeta::new("native", "finish", "test", "mock", "fara"),
    )
    .unwrap();

    let trace = e
        .run_with(
            "finish",
            RunOptions {
                recorder: Some(&recorder),
                ..RunOptions::default()
            },
        )
        .await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    let request = &model.model.seen()[0];
    let hash = kai_core::recorder::fnv1a_hex(&request.system);
    assert_eq!(
        trace.steps[0].call.prompt_system_hash.as_deref(),
        Some(hash.as_str())
    );
    assert_eq!(
        std::fs::read_to_string(root.join("system.txt")).unwrap(),
        request.system
    );
    assert_eq!(
        std::fs::read(root.join("frames/step-000.jpg")).unwrap(),
        *model.sent_image.lock().unwrap()
    );
    assert_eq!(*model.sent_image.lock().unwrap(), unmarked_jpeg());
    assert_eq!(
        std::fs::read_to_string(root.join(format!("systems/{hash}.txt"))).unwrap(),
        request.system
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn recorded_contract_step_matches_actual_model_prompt_and_image() {
    let done = r#"{"thought":"done","actions":[],"message":"complete","phase":"final_answer"}"#;
    let model = CapturingModel {
        model: Counting::new(&[done]),
        sent_image: Mutex::default(),
    };
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.perceptor = &Eyes;
    e.config.oai_contract = Some(CoordFrame::Norm1000);
    e.config.computer_tools_v1 = true;
    let root = std::env::temp_dir().join(format!(
        "kai-contract-trace-{}-{}",
        std::process::id(),
        jiff::Timestamp::now().as_nanosecond()
    ));
    let recorder = TraceRecorder::new(
        &root,
        RunMeta::new("contract", "finish", "test", "mock", "contract"),
    )
    .unwrap();
    let trace = e
        .run_with(
            "finish",
            RunOptions {
                recorder: Some(&recorder),
                ..RunOptions::default()
            },
        )
        .await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    let request = &model.model.seen()[0];
    let hash = kai_core::recorder::fnv1a_hex(&request.system);
    assert_eq!(
        trace.steps[0].call.prompt_system_hash.as_deref(),
        Some(hash.as_str())
    );
    assert_eq!(
        std::fs::read_to_string(root.join("system.txt")).unwrap(),
        request.system
    );
    assert_eq!(
        std::fs::read(root.join("frames/step-000.jpg")).unwrap(),
        *model.sent_image.lock().unwrap()
    );
    assert_eq!(*model.sent_image.lock().unwrap(), unmarked_jpeg());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_repair_trace_records_the_retry_turn_that_produced_the_reply() {
    let model = CapturingModel {
        model: Counting::new(&["malformed", FARA_DONE]),
        sent_image: Mutex::default(),
    };
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let (approve, stop) = (approve_all(), flag());
    let mut e = engine(&model, &screen, &hands, &approve, &stop);
    e.perceptor = &Eyes;
    e.config.native_model = Some(NativeModel::Fara15);
    e.config.computer_tools_v1 = true;
    let root = std::env::temp_dir().join(format!(
        "kai-native-retry-trace-{}-{}",
        std::process::id(),
        jiff::Timestamp::now().as_nanosecond()
    ));
    let recorder = TraceRecorder::new(
        &root,
        RunMeta::new("retry", "finish", "test", "mock", "fara"),
    )
    .unwrap();
    let trace = e
        .run_with(
            "finish",
            RunOptions {
                recorder: Some(&recorder),
                ..RunOptions::default()
            },
        )
        .await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    let seen = model.model.seen();
    assert_eq!(seen.len(), 2);
    assert!(
        seen[1]
            .text
            .contains("previous native tool call was invalid")
    );
    assert_eq!(
        trace.steps[0].call.prompt_user.as_deref(),
        Some(seen[1].text.as_str())
    );
    std::fs::remove_dir_all(root).unwrap();
}
