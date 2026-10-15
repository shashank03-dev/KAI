//! Gemini client field fallbacks (thinkingLevel, responseJsonSchema) against a
//! scripted local server. No live Gemini key is available.

mod client_support;

use client_support::{Reply, Server};
use kai_core::Decision;
use kai_core::client::{GeminiClient, Images, ModelClient, ModelRequest};
use serde_json::json;

fn ok() -> serde_json::Value {
    json!({"candidates":[{"content":{"parts":[
        {"text":"thinking...","thought":true},
        {"text":"{\"thought\":\"ok\",\"action\":\"done\",\"risk\":\"none\"}"}
    ]}}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5}})
}

#[tokio::test]
async fn rejected_fields_are_dropped_and_remembered() {
    let (srv, base) = Server::start(vec![
        Reply::Json(
            400,
            json!({"error":{"message":"Unknown name \"thinkingConfig\""}}),
        ),
        Reply::Json(
            400,
            json!({"error":{"message":"Unknown name \"responseJsonSchema\""}}),
        ),
        Reply::Json(200, ok()),
        Reply::Json(200, ok()),
    ])
    .await;
    let c = GeminiClient::new("k").with_base(&base);
    let imgs = Images {
        screenshot_jpeg: vec![1],
        crop_jpeg: None,
    };
    let schema = Decision::json_schema();
    let req = ModelRequest {
        observation_id: None,
        computer_result: None,
        system: "s",
        user_text: "u",
        images: &imgs,
        schema: &schema,
        max_tokens: 64,
        temperature: 0.0,
    };
    let r = c.decide(&req).await.unwrap();
    assert!(Decision::parse_checked(&r.raw).is_ok(), "{}", r.raw);
    c.decide(&req).await.unwrap();
    let b = srv.bodies();
    assert_eq!(b.len(), 4);
    assert!(
        b[0].0
            .ends_with("/models/gemma-4-26b-a4b-it:generateContent")
    );
    let g = |i: usize| b[i].1["generationConfig"].clone();
    assert_eq!(g(0)["thinkingConfig"]["thinkingLevel"], "minimal");
    assert!(g(0)["responseJsonSchema"].is_object());
    assert!(g(1).get("thinkingConfig").is_none());
    assert!(g(1)["responseJsonSchema"].is_object());
    assert!(g(2).get("responseJsonSchema").is_none());
    assert_eq!(g(2)["responseMimeType"], "application/json");
    assert!(g(3).get("thinkingConfig").is_none() && g(3).get("responseJsonSchema").is_none());
}
