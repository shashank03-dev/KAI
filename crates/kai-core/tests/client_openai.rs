//! OpenAI-compatible client against a scripted local server: structured-output
//! fallback, reasoning-effort fallback, free-tier 403, streaming TTFT, early
//! stop, and vLLM streaming.

mod client_support;

use client_support::{Reply, Server, delta};
use kai_core::client::{ClientError, Images, ModelClient, ModelRequest, OpenAiClient, VllmClient};
use kai_core::{Backend, Decision};
use serde_json::json;
use std::time::{Duration, Instant};

const DONE: &str = r#"{"thought":"ok","action":"done","risk":"none"}"#;

fn completion(content: &str) -> serde_json::Value {
    json!({
        "choices": [{"index":0,"message":{"role":"assistant","content":content}}],
        "usage": {"prompt_tokens": 1200, "completion_tokens": 30, "cost": 0.00042}
    })
}

async fn decide(c: &dyn ModelClient) -> Result<kai_core::client::ModelReply, ClientError> {
    let imgs = Images {
        screenshot_jpeg: vec![0xFF, 0xD8, 1],
        crop_jpeg: None,
    };
    let schema = Decision::json_schema();
    c.decide(&ModelRequest {
        observation_id: None,
        computer_result: None,
        system: "SYS",
        user_text: "task",
        images: &imgs,
        schema: &schema,
        max_tokens: 192,
        temperature: 0.0,
    })
    .await
}

#[tokio::test]
async fn plain_reply_with_usage_and_cost() {
    let (srv, base) = Server::start(vec![Reply::Json(200, completion(DONE))]).await;
    let c = OpenAiClient::gateway("k")
        .with_base(format!("{base}/v1"))
        .with_streaming(false);
    let r = decide(&c).await.unwrap();
    assert_eq!(r.backend, Backend::OpenAi);
    assert_eq!(c.backend(), Backend::OpenAi);
    assert!(Decision::parse_checked(&r.raw).is_ok());
    assert_eq!(
        (r.prompt_tokens, r.completion_tokens),
        (Some(1200), Some(30))
    );
    assert_eq!(r.cost_usd, Some(0.00042));
    assert!(r.first_token.is_none());
    let (path, body) = &srv.bodies()[0];
    assert_eq!(path, "/v1/chat/completions");
    assert_eq!(body["model"], "google/gemma-4-26b-a4b-it");
    assert_eq!(body["messages"][1]["content"][1]["type"], "image_url");
}

#[tokio::test]
async fn schema_rejection_steps_down_and_is_remembered() {
    let bad = json!({"error":{"message":"Invalid schema for response_format 'kai_reply': strict"}});
    let (srv, base) = Server::start(vec![
        Reply::Json(400, bad.clone()),
        Reply::Json(400, bad),
        Reply::Json(200, completion(DONE)),
        Reply::Json(200, completion(DONE)),
    ])
    .await;
    let c = OpenAiClient::openai("k")
        .with_base(format!("{base}/v1"))
        .with_model("gpt-4.1-mini")
        .with_streaming(false);
    decide(&c).await.unwrap();
    decide(&c).await.unwrap();
    let b = srv.bodies();
    assert_eq!(b.len(), 4);
    assert_eq!(b[0].1["response_format"]["json_schema"]["strict"], true);
    assert_eq!(b[1].1["response_format"]["json_schema"]["strict"], false);
    assert!(b[2].1.get("response_format").is_none());
    // Remembered: the next call starts where the last one ended.
    assert!(b[3].1.get("response_format").is_none());
    assert!(
        b[3].1["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("JSON schema")
    );
}

#[tokio::test]
async fn reasoning_effort_rejection_steps_down() {
    let bad = json!({"error":{"message":"Unsupported value: 'minimal' is not supported. param: reasoning_effort"}});
    let (srv, base) = Server::start(vec![
        Reply::Json(400, bad),
        Reply::Json(200, completion(DONE)),
    ])
    .await;
    let c = OpenAiClient::gateway("k")
        .with_base(format!("{base}/v1"))
        .with_model("openai/gpt-5-mini")
        .with_streaming(false);
    decide(&c).await.unwrap();
    let b = srv.bodies();
    assert_eq!(b[0].1["reasoning_effort"], "minimal");
    assert_eq!(b[1].1["reasoning_effort"], "low");
    assert!(b[1].1["max_completion_tokens"].as_u64().unwrap() > 192);
}

#[tokio::test]
async fn free_tier_403_is_unavailable_and_fails_over() {
    let body = json!({"error":{"message":"Free tier users do not have access to this model.",
        "type":"no_providers_available"}});
    let (_srv, base) = Server::start(vec![Reply::Json(403, body)]).await;
    let c = OpenAiClient::gateway("k")
        .with_base(format!("{base}/v1"))
        .with_model("google/gemma-4-31b-it");
    let e = decide(&c).await.err().unwrap();
    assert!(matches!(e, ClientError::Unavailable(_)), "{e}");
    assert!(e.is_failover());
    let e = decide(&OpenAiClient::gateway("")).await.err().unwrap();
    assert!(matches!(e, ClientError::MissingKey(_)));
}

fn sse(delay_before_tail: Duration) -> Reply {
    let usage = "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":7,\"cost\":0.001}}\n\n";
    Reply::Sse(vec![
        (
            Duration::ZERO,
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n".into(),
        ),
        (
            Duration::from_millis(150),
            delta("```json\n{\"thought\":\"ok\","),
        ),
        (
            Duration::from_millis(20),
            delta("\"action\":\"done\",\"risk\":\"none\"}"),
        ),
        (delay_before_tail, delta("\n```")),
        (Duration::ZERO, usage.into()),
        (Duration::ZERO, "data: [DONE]\n\n".into()),
    ])
}

#[tokio::test]
async fn streaming_measures_ttft_and_reads_usage() {
    let (srv, base) = Server::start(vec![sse(Duration::from_millis(10))]).await;
    let c = OpenAiClient::gateway("k").with_base(format!("{base}/v1"));
    let r = decide(&c).await.unwrap();
    assert!(Decision::parse_checked(&r.raw).is_ok(), "{}", r.raw);
    let ttft = r.first_token.unwrap();
    assert!(
        ttft >= Duration::from_millis(140) && ttft < r.total,
        "{ttft:?}"
    );
    assert_eq!(r.completion_tokens, Some(7));
    assert_eq!(r.cost_usd, Some(0.001));
    assert_eq!(srv.bodies()[0].1["stream"], true);
}

#[tokio::test]
async fn early_stop_returns_when_the_object_closes() {
    let (_srv, base) = Server::start(vec![sse(Duration::from_millis(1500))]).await;
    let c = OpenAiClient::gateway("k")
        .with_base(format!("{base}/v1"))
        .with_early_stop(true);
    let t = Instant::now();
    let r = decide(&c).await.unwrap();
    assert!(
        t.elapsed() < Duration::from_millis(1200),
        "{:?}",
        t.elapsed()
    );
    assert!(Decision::parse_checked(&r.raw).is_ok());
    assert_eq!(r.completion_tokens, None); // usage arrives after the stop
}

#[tokio::test]
async fn vllm_streams_too() {
    let (srv, base) = Server::start(vec![sse(Duration::ZERO)]).await;
    let c = VllmClient::new(format!("{base}/v1"), "k");
    let r = decide(&c).await.unwrap();
    assert_eq!(r.backend, Backend::Vllm);
    assert!(r.first_token.is_some());
    assert_eq!(srv.bodies()[0].1["stream_options"]["include_usage"], true);
    let (_srv, base) = Server::start(vec![Reply::Json(200, completion(DONE))]).await;
    let c = VllmClient::new(format!("{base}/v1"), "k").with_streaming(false);
    assert_eq!(decide(&c).await.unwrap().prompt_tokens, Some(1200));
}

#[tokio::test]
async fn local_server_needs_no_key_and_reports_local() {
    let (srv, base) = Server::start(vec![Reply::Json(200, completion(DONE))]).await;
    let c = OpenAiClient::local(format!("{base}/v1"), "gemma-4-e4b-it").with_streaming(false);
    let r = decide(&c).await.unwrap();
    assert_eq!(r.backend, Backend::Local);
    let head = srv.heads()[0].to_ascii_lowercase();
    assert!(!head.contains("authorization"), "{head}");
    let (_, body) = &srv.bodies()[0];
    assert_eq!(body["model"], "gemma-4-e4b-it");
    assert_eq!(body["messages"][1]["content"][1]["type"], "image_url");
}
