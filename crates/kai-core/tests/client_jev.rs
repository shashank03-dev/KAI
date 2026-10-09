//! JEV decision client and the verifiers against a scripted local server.

mod client_support;

use client_support::{Reply, Server};
use kai_core::client::jev::JevEndpoint;
use kai_core::client::{Answer, DecisionClient, JevVerifier, LlmVerifier, OpenAiClient, Question};
use kai_core::ports::Verifier;
use serde_json::json;

#[tokio::test]
async fn systemone_noul_answer_becomes_a_probability() {
    let (srv, base) = Server::start(vec![Reply::Json(
        200,
        json!({"answers":{"met":{"type":"noul","noul":0.98}},"usage":{"input_tokens":40}}),
    )])
    .await;
    let v = JevVerifier::new(DecisionClient::gateway("k").with_base(&base));
    let p = v
        .check(
            "save the file",
            "title has no asterisk",
            "#1 button \"Save\"",
        )
        .await;
    assert_eq!(p, Some(0.98));
    let (path, body) = &srv.bodies()[0];
    assert_eq!(path, "/typesafe/v1/systemone");
    assert_eq!(body["model"], "typesafe-ai/jev");
    assert_eq!(body["questions"]["met"]["type"], "noul");
    assert!(
        body["state"]
            .as_str()
            .unwrap()
            .contains("title has no asterisk")
    );
}

#[tokio::test]
async fn evaluate_shape_with_typed_questions() {
    let (srv, base) = Server::start(vec![Reply::Json(
        200,
        json!({"answers":{
            "ok":{"type":"boolean","probability":0.3},
            "next":{"type":"choice","choice":"retry"},
            "q":{"type":"score","score":4.0}
        }}),
    )])
    .await;
    let c = DecisionClient::gateway("k")
        .with_base(&base)
        .with_endpoint(JevEndpoint::Evaluate);
    let a = c
        .ask(
            "state",
            &[
                (
                    "ok",
                    Question::Noul {
                        instructions: "ok?".into(),
                    },
                ),
                (
                    "next",
                    Question::Choice {
                        instructions: "next?".into(),
                        options: vec!["retry".into(), "stop".into()],
                    },
                ),
                (
                    "q",
                    Question::Score {
                        instructions: "quality".into(),
                        criteria: Some("1-5".into()),
                    },
                ),
            ],
        )
        .await
        .unwrap();
    assert_eq!(a["ok"].probability(), Some(0.3));
    assert_eq!(
        a["next"],
        Answer::Choice {
            choice: "retry".into()
        }
    );
    assert_eq!(a["q"], Answer::Score(4.0));
    let (path, body) = &srv.bodies()[0];
    assert_eq!(path, "/v1/evaluate");
    assert_eq!(body["questions"]["ok"]["type"], "boolean");
    assert_eq!(body["questions"]["next"]["options"][1], "stop");
}

#[tokio::test]
async fn free_tier_turns_the_verifier_off() {
    let body = json!({"error":{"message":"Free tier users do not have access to this model.",
        "type":"no_providers_available"}});
    let (srv, base) = Server::start(vec![Reply::Json(403, body)]).await;
    let c = DecisionClient::gateway("k").with_base(&base);
    let v = JevVerifier::new(c);
    assert_eq!(v.check("t", "a dialog", "s").await, None);
    assert!(v.is_off());
    // Off: no second request.
    assert_eq!(v.check("t", "a dialog", "s").await, None);
    assert_eq!(srv.bodies().len(), 1);
    let e = DecisionClient::gateway("k")
        .with_base(&base)
        .ask("s", &[])
        .await
        .err()
        .unwrap();
    assert!(e.to_string().contains("500")); // script exhausted, not Unavailable
}

#[tokio::test]
async fn llm_verifier_over_openai_client() {
    let (srv, base) = Server::start(vec![Reply::Json(
        200,
        json!({"choices":[{"message":{"content":"{\"p\":0.7}"}}]}),
    )])
    .await;
    let c = OpenAiClient::gateway("k")
        .with_base(format!("{base}/v1"))
        .with_model("openai/gpt-4.1-nano")
        .with_streaming(false);
    let v = LlmVerifier::new(Box::new(c));
    assert_eq!(v.check("t", "the file is saved", "s").await, Some(0.7));
    let body = &srv.bodies()[0].1;
    assert_eq!(body["messages"][1]["content"].as_array().unwrap().len(), 1); // text only
    assert_eq!(
        body["response_format"]["json_schema"]["schema"]["required"][0],
        "p"
    );
}
