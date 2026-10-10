//! jevos-v4 served locally (`DecisionClient::local`), against a scripted
//! server: path, optional auth, choice/score parsing, health.

mod client_support;

use client_support::{Reply, Server};
use kai_core::client::jev::JevEndpoint;
use kai_core::client::{Answer, DecisionClient, Question};
use kai_core::executor::Decider;
use serde_json::json;

fn questions() -> Vec<(String, Question)> {
    vec![
        (
            "target".into(),
            Question::Criteria {
                instructions: "Which element is the Email field?".into(),
                criteria: vec![
                    ("3".into(), "#3 textfield \"Email\"".into()),
                    ("none".into(), "None of these".into()),
                ],
            },
        ),
        (
            "ready".into(),
            Question::Noul {
                instructions: "Can it be typed now?".into(),
            },
        ),
        (
            "mood".into(),
            Question::Levels {
                instructions: "How does the user feel?".into(),
                levels: vec!["calm".into(), "annoyed".into()],
            },
        ),
    ]
}

#[tokio::test]
async fn local_endpoint_wire_format_without_key() {
    let (srv, base) = Server::start(vec![Reply::Json(
        200,
        json!({"model":"jevos-v4","answers":{
            "target":{"type":"choice","choice":"3","probabilities":{"3":0.91,"none":0.09},
                      "confidence":0.91},
            "ready":{"type":"noul","noul":0.94},
            "mood":{"type":"score","level":"calm","probabilities":{"calm":0.7,"annoyed":0.3},
                    "confidence":0.7}
        },"usage":{"input_tokens":120}}),
    )])
    .await;
    let c = DecisionClient::local(format!("{base}/"));
    assert_eq!(c.endpoint(), JevEndpoint::Local);
    let state = json!({"task":"contact form","elements":"#3 textfield \"Email\""});
    let a = Decider::ask(&c, &state, &questions()).await.unwrap();
    assert_eq!(a["target"].option(), Some("3"));
    assert!((a["target"].margin().unwrap() - 0.82).abs() < 1e-9);
    assert_eq!(a["ready"].probability(), Some(0.94));
    assert!(matches!(&a["mood"], Answer::Score { level: Some(l), .. } if l == "calm"));

    let (path, body) = &srv.bodies()[0];
    assert_eq!(path, "/v1/systemone", "no /typesafe prefix");
    assert_eq!(body["model"], "jev-latest");
    assert_eq!(
        body["state"]["task"], "contact form",
        "state sent as an object"
    );
    let q = &body["questions"];
    assert_eq!(q["target"]["type"], "choice");
    assert_eq!(q["target"]["criteria"]["3"], "#3 textfield \"Email\"");
    assert_eq!(q["ready"]["type"], "noul");
    assert_eq!(q["mood"]["type"], "score");
    assert_eq!(q["mood"]["criteria"][1], "annoyed");
    let head = srv.heads()[0].to_ascii_lowercase();
    assert!(!head.contains("authorization"), "no key, no auth header");
}

#[tokio::test]
async fn local_endpoint_sends_a_key_only_when_set_and_tolerates_missing_fields() {
    let (srv, base) = Server::start(vec![Reply::Json(
        200,
        json!({"answers":{
            // No probabilities, no confidence; another option field name.
            "target":{"type":"choice","answer":"none"},
            // No type at all.
            "ready":{"noul":0.2},
            // Unknown type: dropped, not guessed.
            "mood":{"type":"vibes","x":1}
        }}),
    )])
    .await;
    let c = DecisionClient::local(&base).with_key("local-test-key");
    let a = Decider::ask(&c, &json!("plain text state"), &questions())
        .await
        .unwrap();
    assert_eq!(a["target"].option(), Some("none"));
    assert_eq!(a["target"].confidence(), None);
    assert_eq!(a["target"].margin(), None);
    assert_eq!(a["ready"].probability(), Some(0.2));
    assert!(!a.contains_key("mood"));
    let head = srv.heads()[0].to_ascii_lowercase();
    assert!(head.contains("authorization: bearer local-test-key"));
    assert_eq!(srv.bodies()[0].1["state"], "plain text state");
}

#[tokio::test]
async fn health_measures_the_round_trip_and_errors_are_typed() {
    let (srv, base) = Server::start(vec![
        Reply::Json(200, json!({"status":"ok"})),
        Reply::Json(503, json!({"status":"loading"})),
    ])
    .await;
    let c = DecisionClient::local(&base);
    let t = Decider::health(&c).await.unwrap();
    assert!(t.as_millis() < 2000);
    assert_eq!(srv.bodies()[0].0, "/health");
    assert!(Decider::health(&c).await.is_err());
    // Nothing listening: a transport error, quickly.
    let dead = DecisionClient::local("http://127.0.0.1:9");
    assert!(dead.health().await.is_err());
}
