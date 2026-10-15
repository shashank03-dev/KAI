mod common;

use async_trait::async_trait;
use common::{Counting, approve_all, flag};
use kai_core::engine::NoSteering;
use kai_core::*;
use std::sync::Mutex;

const CODE: &str = "482913";
const DONE: &str = r#"{"thought":"done","action":"done","risk":"none"}"#;

struct Page {
    value: Mutex<String>,
    fail_typing: bool,
}

#[async_trait]
impl Perceptor for Page {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let value = self.value.lock().unwrap().clone();
        let el = |mark, role, name: &str, value| Element {
            mark, role, name: name.into(), value,
            bounds: Rect::new(10., mark as f32 * 25., 100., 20.),
            enabled: true, secure: false, focused: false,
        };
        Ok(Observation {
            images: Images {
                // Real JPEG bytes: the policy must suppress the entire image,
                // independently of what pixels an application chooses to draw.
                screenshot_jpeg: include_bytes!("../../../docs/img/set-of-marks-gtk.jpg").to_vec(),
                crop_jpeg: Some(vec![1, 2, 3]),
            },
            grounding: Grounding::Marks(vec![
                el(1, Role::Other, "We sent a code to your phone", None),
                el(2, Role::TextField, "Verification code", Some(value)),
                el(3, Role::Button, "Verify", None),
            ]),
            screen_hash: 1, timing: Default::default(), screen: None,
            window_title: None, url: None,
        })
    }
}

#[async_trait]
impl Actuator for Page {
    async fn act(&self, d: &Decision, _: &Grounding) -> anyhow::Result<()> {
        if d.action == Action::Type {
            *self.value.lock().unwrap() = if self.fail_typing {
                CODE[..3].into()
            } else { d.text.clone().unwrap() };
            if self.fail_typing { anyhow::bail!("driver echoed {CODE}"); }
        }
        Ok(())
    }
    async fn open_app(&self, _: &str) -> anyhow::Result<()> { Ok(()) }
    async fn open_url(&self, _: &str) -> anyhow::Result<()> { Ok(()) }
    async fn shell(&self, _: &str) -> anyhow::Result<String> { Ok(String::new()) }
}

struct Person;
#[async_trait]
impl HumanHelper for Person {
    async fn ask(&self, _: &HumanCheck) -> HumanReply {
        HumanReply::Code(OneTimeCode::parse(CODE).unwrap())
    }
}

struct Sink<'a> {
    page: &'a Page,
    leaked_images: Mutex<usize>,
    text: Mutex<Vec<String>>,
}
impl EventSink for Sink<'_> {
    fn emit(&self, event: &EngineEvent) {
        if !self.page.value.lock().unwrap().is_empty()
            && let EngineEvent::Observed { screenshot, .. } = event
            && !screenshot.is_empty()
        {
            *self.leaked_images.lock().unwrap() += 1;
        }
        self.text.lock().unwrap().push(format!("{event:?}"));
    }
}

async fn check(fail_typing: bool) {
    let page = Page { value: Mutex::new(String::new()), fail_typing };
    let model = Counting::new(&[DONE]);
    let sink = Sink { page: &page, leaked_images: Mutex::new(0), text: Mutex::new(Vec::new()) };
    let approve = approve_all();
    let stop = flag();
    let root = std::env::temp_dir().join(format!(
        "kai-privacy-{}-{}", std::process::id(), jiff::Timestamp::now().as_nanosecond()
    ));
    let recorder = TraceRecorder::new(&root,
        RunMeta::new("privacy", "Sign in", "test", "mock", "mock")
    ).unwrap();
    let trace = Engine {
        client: &model, careful: None, perceptor: &page, actuator: &page,
        approver: &approve, interrupt: &stop, sink: &sink,
        steering: &NoSteering, verifier: None, human: Some(&Person), skills: &[],
        config: Config { tree_only: false, max_steps: 5, ..Config::default() },
    }.run_with("Sign in", RunOptions { recorder: Some(&recorder), ..RunOptions::default() }).await;
    assert_eq!(*sink.leaked_images.lock().unwrap(), 0, "UI/recording boundary leaked pixels");
    if fail_typing {
        assert_eq!(trace.outcome, Some(Outcome::AskedUser));
    } else {
        assert!(!model.seen().is_empty(), "the model must be able to continue from safe text");
    }
    for seen in model.seen() {
        assert!(!seen.had_image && !seen.had_crop, "model saw secret-bearing pixels");
        assert!(!seen.text.contains(CODE));
        if fail_typing { assert!(!seen.text.contains("482"), "partial field value leaked"); }
    }
    assert!(!serde_json::to_string(&trace).unwrap().contains(CODE));
    assert!(!sink.text.lock().unwrap().join("\n").contains(CODE));
    for entry in std::fs::read_dir(root.join("frames")).unwrap() {
        panic!("sensitive run persisted a frame: {:?}", entry.unwrap().path());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn successful_sensitive_typing_suppresses_future_pixels() { check(false).await; }

#[tokio::test]
async fn partially_failed_sensitive_typing_still_suppresses_pixels_and_values() { check(true).await; }
