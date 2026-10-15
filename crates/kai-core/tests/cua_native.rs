//! OpenAI's native computer-use loop against a scripted /v1/responses server
//! (no OpenAI key is available, so it cannot run live).

mod client_support;

use async_trait::async_trait;
use client_support::{Reply, Server};
use kai_core::client::ClientError;
use kai_core::cua::{CuaConfig, CuaRunner, Mapped, jpeg_size, map_action};
use kai_core::engine::{NeverStops, NullSink};
use kai_core::{
    Action, Actuator, Decision, Grounding, Images, Observation, ObserveTiming, Outcome, Perceptor,
    Rect,
};
use serde_json::json;
use std::sync::Mutex;

const JPG: &[u8] = include_bytes!("../../../docs/img/set-of-marks-gtk.jpg");

struct Screen;
#[async_trait]
impl Perceptor for Screen {
    async fn observe(&self) -> anyhow::Result<Observation> {
        Ok(Observation {
            images: Images {
                screenshot_jpeg: JPG.to_vec(),
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(vec![kai_core::Element {
                mark: 1,
                role: kai_core::Role::TextField,
                name: "Editor".into(),
                bounds: Rect::new(1., 1., 5., 5.),
                enabled: true,
                focused: true,
                secure: false,
                value: None,
            }]),
            screen_hash: 0,
            timing: ObserveTiming::default(),
            // The display is 2x the screenshot, offset by a monitor.
            screen: Some(Rect::new(100.0, 0.0, 2560.0, 1440.0)),
            window_title: None,
            url: None,
        })
    }
}

type Acted = (Action, Option<(f32, f32)>, Option<String>);

#[derive(Default)]
struct Hands(Mutex<Vec<Acted>>);
#[async_trait]
impl Actuator for Hands {
    fn computer_capabilities(&self) -> kai_core::computer::Capabilities {
        kai_core::computer::Capabilities::all()
    }
    async fn computer_act(
        &self,
        a: &kai_core::computer::ComputerAction,
        s: &kai_core::computer::ObservationSpace,
        _: &Grounding,
        c: &kai_core::computer::ExecutionContext,
    ) -> Result<(), kai_core::computer::ToolError> {
        use kai_core::computer::*;
        c.check()?;
        let item = match a {
            ComputerAction::Click {
                target: Target::Point { x, y, .. },
                ..
            } => {
                let (x, y) = s.point(*x, *y)?;
                (Action::Click, Some((x as f32, y as f32)), None)
            }
            ComputerAction::Type { text, .. } => (Action::Type, None, Some(text.clone())),
            ComputerAction::Keypress { .. } => (Action::Key, None, None),
            ComputerAction::Move { .. } => (Action::Computer, None, None),
            _ => return Err(ToolError::Unsupported),
        };
        self.0.lock().unwrap().push(item);
        Ok(())
    }

    async fn act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        let at = d.target.and_then(|t| g.target_rect(t)).map(|r| r.center());
        self.0.lock().unwrap().push((d.action, at, d.text.clone()));
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

fn call(id: &str, action: serde_json::Value, checks: serde_json::Value) -> serde_json::Value {
    json!({"id": format!("resp_{id}"), "output": [
        {"type":"reasoning","id":"rs","summary":[]},
        {"type":"computer_call","id":"cu","call_id": format!("call_{id}"),
         "action": action, "pending_safety_checks": checks, "status":"completed"}
    ]})
}

fn runner<'a>(base: &str, key: Option<&str>, hands: &'a Hands) -> CuaRunner<'a> {
    CuaRunner {
        config: CuaConfig {
            base: format!("{base}/v1"),
            api_key: key.map(str::to_string),
            ..Default::default()
        },
        perceptor: &Screen,
        actuator: hands,
        interrupt: &NeverStops,
        sink: &NullSink,
    }
}

#[tokio::test]
async fn loops_on_computer_calls_until_a_message() {
    let (srv, base) = Server::start(vec![
        Reply::Json(
            200,
            call(
                "1",
                json!({"type":"click","x":640,"y":360,"button":"left"}),
                json!([]),
            ),
        ),
        Reply::Json(
            200,
            call("2", json!({"type":"type","text":"hello"}), json!([])),
        ),
        Reply::Json(
            200,
            call(
                "3",
                json!({"type":"keypress","keys":["CTRL","S"]}),
                json!([]),
            ),
        ),
        Reply::Json(
            200,
            json!({"id":"resp_4","output":[{"type":"message","role":"assistant",
            "content":[{"type":"output_text","text":"Saved the file."}]}]}),
        ),
    ])
    .await;
    let hands = Hands::default();
    let run = runner(&base, Some("sk-test"), &hands)
        .run_with_approver("type hello and save", Some(&Permit))
        .await
        .unwrap();
    assert_eq!(run.trace.outcome, Some(Outcome::AskedUser));
    assert_eq!(run.trace.steps.len(), 4);
    assert_eq!(
        run.trace.steps.last().unwrap().decision.action,
        Action::AskUser
    );
    assert_eq!(run.message.as_deref(), Some("Saved the file."));
    assert!(run.trace.steps[0].backend.contains("native"));

    let acts = hands.0.lock().unwrap().clone();
    // (640, 360) in the 1280x720 screenshot -> 2x scale + 100 px offset.
    assert_eq!(acts[0], (Action::Click, Some((1380.0, 720.0)), None));
    assert_eq!(acts[1].0, Action::Type);
    assert_eq!(acts[1].2.as_deref(), Some("hello"));
    assert_eq!(acts[2].0, Action::Key);

    let b = srv.bodies();
    assert_eq!(b[0].0, "/v1/responses");
    assert_eq!(b[0].1["model"], "computer-use-preview");
    assert_eq!(b[0].1["truncation"], "auto");
    let tool = &b[0].1["tools"][0];
    assert_eq!(
        (
            tool["type"].as_str(),
            tool["display_width"].as_u64(),
            tool["display_height"].as_u64()
        ),
        (Some("computer_use_preview"), Some(1280), Some(720))
    );
    assert!(
        b[0].1["input"][0]["content"][1]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/jpeg;base64,")
    );
    assert_eq!(b[1].1["previous_response_id"], "resp_1");
    let out = &b[1].1["input"][0];
    assert_eq!(out["type"], "computer_call_output");
    assert_eq!(out["call_id"], "call_1");
    assert_eq!(out["output"]["type"], "input_image");
}

#[tokio::test]
async fn safety_checks_stop_and_ask() {
    let (_srv, base) = Server::start(vec![Reply::Json(
        200,
        call(
            "1",
            json!({"type":"click","x":1,"y":1}),
            json!([{"id":"sc","code":"malicious_instructions","message":"Suspicious page text."}]),
        ),
    )])
    .await;
    let hands = Hands::default();
    let run = runner(&base, Some("sk"), &hands).run("t").await.unwrap();
    assert_eq!(run.trace.outcome, Some(Outcome::AskedUser));
    assert!(run.message.unwrap().contains("Suspicious page text."));
    assert!(hands.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn missing_key_and_api_errors_are_clear() {
    let hands = Hands::default();
    let e = runner("http://127.0.0.1:9", None, &hands)
        .run("t")
        .await
        .err()
        .unwrap();
    assert!(matches!(e, ClientError::MissingKey(_)));
    assert!(e.to_string().contains("OPENAI_API_KEY"));

    let (_srv, base) = Server::start(vec![Reply::Json(
        404,
        json!({"error":{"message":"Model 'computer-use-preview' not found","type":"model_not_found"}}),
    )])
    .await;
    let run = runner(&base, Some("sk"), &hands).run("t").await.unwrap();
    assert_eq!(run.trace.outcome, Some(Outcome::Failed));
    assert!(run.error.unwrap().contains("not available"));
}

#[test]
fn clicks_map_from_image_to_screen_pixels() {
    let screen = Rect::new(1920.0, 0.0, 1920.0, 1080.0);
    let a = json!({"type":"click","x":640,"y":360,"button":"left"});
    let Mapped::Act(d, g) = map_action(&a, (1280, 720), screen) else {
        panic!()
    };
    assert_eq!(d.action, Action::Click);
    let (x, y) = g.target_rect(d.target.unwrap()).unwrap().center();
    assert_eq!((x, y), (1920.0 + 960.0, 540.0));
}

#[test]
fn scroll_keys_and_unsupported() {
    let s = Rect::new(0.0, 0.0, 100.0, 100.0);
    let a = json!({"type":"scroll","x":1,"y":1,"scroll_x":0,"scroll_y":-300});
    assert!(matches!(map_action(&a, (100, 100), s), Mapped::Ask(_)));
    let a = json!({"type":"keypress","keys":["CTRL","ARROWLEFT"]});
    let Mapped::Act(d, _) = map_action(&a, (100, 100), s) else {
        panic!()
    };
    assert_eq!(d.keys, vec!["ctrl", "left"]);
    let a = json!({"type":"drag","path":[{"x":1,"y":1}]});
    assert!(matches!(map_action(&a, (100, 100), s), Mapped::Ask(_)));
    assert!(matches!(
        map_action(&json!({"type":"teleport"}), (1, 1), s),
        Mapped::Ask(_)
    ));
}

#[test]
fn jpeg_size_reads_sof() {
    let jpg = JPG;
    let (w, h) = jpeg_size(jpg).unwrap();
    assert!(w > 100 && h > 100);
    assert_eq!(jpeg_size(b"nope"), None);
}

#[test]
fn legacy_runner_never_approximates_unavailable_primitives_or_bad_coordinates() {
    let s = Rect::new(0., 0., 100., 100.);
    for a in [
        json!({"type":"move","x":1,"y":1}),
        json!({"type":"scroll","x":1,"y":1,"scroll_x":0,"scroll_y":0}),
        json!({"type":"click","x":1,"y":1,"button":"mystery"}),
        json!({"type":"click","x":-1,"y":1,"button":"left"}),
        json!({"type":"click","x":1,"button":"left"}),
    ] {
        assert!(
            matches!(map_action(&a, (100, 100), s), Mapped::Ask(_)),
            "{a}"
        );
    }
}

struct Permit;
#[async_trait]
impl kai_core::Approver for Permit {
    async fn approve(&self, _: &Decision, _: &Grounding) -> kai_core::ApprovalChoice {
        kai_core::ApprovalChoice::Approve
    }
}

#[tokio::test]
async fn current_action_array_uses_shared_executor_and_computer_screenshot_output() {
    let (server,base)=Server::start(vec![
        Reply::Json(200,json!({"id":"r1","status":"completed","output":[{"type":"computer_call","call_id":"c1","actions":[{"type":"move","x":10,"y":20},{"type":"click","x":20,"y":30,"button":"left"}],"pending_safety_checks":[]}]})),
        Reply::Json(200,json!({"id":"r2","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"done"}]}]}))
    ]).await;
    let hands = Hands::default();
    let mut r = runner(&base, Some("fixture"), &hands);
    r.config.computer_tools_v1 = true;
    let run = r
        .run_with_approver("move and click", Some(&Permit))
        .await
        .unwrap();
    assert_eq!(run.trace.outcome, Some(Outcome::AskedUser));
    assert_eq!(
        hands
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|a| a.0)
            .collect::<Vec<_>>(),
        vec![Action::Computer, Action::Click]
    );
    assert_eq!(run.trace.steps[0].path.as_deref(), Some("computer"));
    let bodies = server.bodies();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0].1["tools"][0]["type"], "computer");
    assert_eq!(bodies[1].1["input"][0]["call_id"], "c1");
    assert_eq!(
        bodies[1].1["input"][0]["output"]["type"],
        "computer_screenshot"
    );
}

#[tokio::test]
async fn native_side_effect_requires_host_approval() {
    let (_, base) = Server::start(vec![Reply::Json(
        200,
        call(
            "1",
            json!({"type":"click","x":10,"y":20,"button":"left"}),
            json!([]),
        ),
    )])
    .await;
    let hands = Hands::default();
    let run = runner(&base, Some("fixture"), &hands)
        .run("click")
        .await
        .unwrap();
    assert_eq!(run.trace.outcome, Some(Outcome::Stopped));
    assert!(hands.0.lock().unwrap().is_empty());
}

struct SkipFirst(std::sync::atomic::AtomicUsize);
#[async_trait]
impl kai_core::Approver for SkipFirst {
    async fn approve(&self, _: &Decision, _: &Grounding) -> kai_core::ApprovalChoice {
        if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            kai_core::ApprovalChoice::Skip
        } else {
            kai_core::ApprovalChoice::Approve
        }
    }
}
#[tokio::test]
async fn provider_batch_stops_after_skip_and_preflights_later_coordinates() {
    for (actions, skip) in [
        (
            json!([{"type":"click","x":20,"y":30,"button":"left"},{"type":"type","text":"dependent text"}]),
            true,
        ),
        (
            json!([{"type":"move","x":10,"y":20},{"type":"click","x":9999,"y":30,"button":"left"}]),
            false,
        ),
    ] {
        let (_,base)=Server::start(vec![Reply::Json(200,json!({"id":"r","status":"completed","output":[{"type":"computer_call","call_id":"c","actions":actions}]})),Reply::Json(200,json!({"id":"done","output":[]}))]).await;
        let hands = Hands::default();
        let approver = SkipFirst(std::sync::atomic::AtomicUsize::new(if skip {
            0
        } else {
            1
        }));
        let run = runner(&base, Some("fixture"), &hands)
            .run_with_approver("batch", Some(&approver))
            .await
            .unwrap();
        assert!(
            hands.0.lock().unwrap().is_empty(),
            "batch mutated despite skipped/invalid predecessor: {:?}",
            run.trace.outcome
        );
    }
}

struct ChangedScreen<'a>(&'a Hands);
#[async_trait]
impl Perceptor for ChangedScreen<'_> {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let mut obs = Screen.observe().await?;
        if !self.0.0.lock().unwrap().is_empty()
            && let Grounding::Marks(els) = &mut obs.grounding
        {
            els[0].name = "A different layout".into();
        }
        Ok(obs)
    }
}
#[tokio::test]
async fn queued_provider_coordinates_cannot_be_rebound_to_a_changed_layout() {
    let (_,base)=Server::start(vec![Reply::Json(200,json!({"id":"r","output":[{"type":"computer_call","call_id":"c","actions":[{"type":"move","x":10,"y":20},{"type":"click","x":20,"y":30,"button":"left"}]}]}))]).await;
    let hands = Hands::default();
    let screen = ChangedScreen(&hands);
    let mut r = runner(&base, Some("fixture"), &hands);
    r.perceptor = &screen;
    let run = r.run_with_approver("batch", Some(&Permit)).await.unwrap();
    assert_eq!(run.trace.outcome, Some(Outcome::AskedUser));
    assert_eq!(hands.0.lock().unwrap().len(), 1);
}
