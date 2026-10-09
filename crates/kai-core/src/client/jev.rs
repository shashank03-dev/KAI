//! TypeSafe AI's JEV ("System One") decision model over the AI Gateway: typed
//! questions about a text state, answered with calibrated numbers in one fast,
//! cheap call. Used as a [`Verifier`] for a step's `expect`.
//!
//! Two endpoint shapes exist (both measured to validate requests):
//! - `POST {base}/v1/evaluate`: question types `boolean | choice | score`.
//! - `POST {base}/typesafe/v1/systemone`: also `noul` (a probability).
//!
//! On the Gateway free tier both answer 403 "Free tier users do not have
//! access"; that surfaces as [`ClientError::Unavailable`], and the verifier
//! turns itself off and returns `None` so the loop proceeds without it.

use super::{ClientError, http_client, send_error};
use crate::ports::Verifier;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// One typed question.
#[derive(Debug, Clone, PartialEq)]
pub enum Question {
    /// Probability that the statement holds (systemone only; sent as
    /// `boolean` on `/v1/evaluate`).
    Noul {
        instructions: String,
    },
    Boolean {
        instructions: String,
    },
    Choice {
        instructions: String,
        options: Vec<String>,
    },
    Score {
        instructions: String,
        criteria: Option<String>,
    },
}

/// One typed answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Noul(f64),
    Boolean { probability: f64 },
    Choice { choice: String },
    Score(f64),
}

impl Answer {
    /// P(yes) for `noul`/`boolean` answers.
    pub fn probability(&self) -> Option<f64> {
        match self {
            Answer::Noul(p) | Answer::Boolean { probability: p } => Some(*p),
            _ => None,
        }
    }

    /// Parse one answer object; tolerant of field-name variants.
    pub fn parse(v: &serde_json::Value) -> Option<Self> {
        let num = |keys: &[&str]| keys.iter().find_map(|k| v[*k].as_f64());
        match v["type"].as_str()? {
            "noul" => num(&["noul", "probability", "value"]).map(Answer::Noul),
            "boolean" => {
                let p = num(&["probability", "noul", "p"]).or_else(|| {
                    v["value"]
                        .as_bool()
                        .or_else(|| v["answer"].as_bool())
                        .map(|b| if b { 1.0 } else { 0.0 })
                })?;
                Some(Answer::Boolean { probability: p })
            }
            "choice" => ["choice", "answer", "value"]
                .iter()
                .find_map(|k| v[*k].as_str())
                .map(|c| Answer::Choice {
                    choice: c.to_string(),
                }),
            "score" => num(&["score", "value"]).map(Answer::Score),
            _ => None,
        }
    }
}

/// Which request shape to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JevEndpoint {
    /// `/v1/evaluate` (no `noul`).
    Evaluate,
    /// `/typesafe/v1/systemone`.
    SystemOne,
}

/// A general client for typed decision questions.
pub struct DecisionClient {
    http: reqwest::Client,
    base: String,
    api_key: String,
    model: String,
    endpoint: JevEndpoint,
    timeout: Duration,
}

impl DecisionClient {
    /// JEV on the Vercel AI Gateway.
    pub fn gateway(api_key: impl Into<String>) -> Self {
        Self {
            http: http_client(),
            base: "https://ai-gateway.vercel.sh".to_string(),
            api_key: api_key.into(),
            model: "typesafe-ai/jev".to_string(),
            endpoint: JevEndpoint::SystemOne,
            timeout: Duration::from_secs(8),
        }
    }

    /// Host root without a path, e.g. `http://127.0.0.1:9000`.
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into().trim_end_matches('/').to_string();
        self
    }

    pub fn with_endpoint(mut self, e: JevEndpoint) -> Self {
        self.endpoint = e;
        self
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    pub(crate) fn body(&self, state: &str, questions: &[(&str, Question)]) -> serde_json::Value {
        let mut qs = serde_json::Map::new();
        for (id, q) in questions {
            let v = match q {
                Question::Noul { instructions } => {
                    let ty = match self.endpoint {
                        JevEndpoint::SystemOne => "noul",
                        JevEndpoint::Evaluate => "boolean",
                    };
                    serde_json::json!({ "type": ty, "instructions": instructions })
                }
                Question::Boolean { instructions } => {
                    serde_json::json!({ "type": "boolean", "instructions": instructions })
                }
                Question::Choice {
                    instructions,
                    options,
                } => serde_json::json!({
                    "type": "choice", "instructions": instructions, "options": options
                }),
                Question::Score {
                    instructions,
                    criteria,
                } => {
                    let mut v =
                        serde_json::json!({ "type": "score", "instructions": instructions });
                    if let Some(c) = criteria {
                        v["criteria"] = c.as_str().into();
                    }
                    v
                }
            };
            qs.insert(id.to_string(), v);
        }
        serde_json::json!({ "model": self.model, "state": state, "questions": qs })
    }

    /// Ask typed questions about `state`. Answers keyed by question id.
    pub async fn ask(
        &self,
        state: &str,
        questions: &[(&str, Question)],
    ) -> Result<BTreeMap<String, Answer>, ClientError> {
        if self.api_key.is_empty() {
            return Err(ClientError::MissingKey("AI_GATEWAY_API_KEY"));
        }
        let path = match self.endpoint {
            JevEndpoint::Evaluate => "/v1/evaluate",
            JevEndpoint::SystemOne => "/typesafe/v1/systemone",
        };
        let resp = self
            .http
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.api_key)
            .json(&self.body(state, questions))
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| send_error(e, self.timeout))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ClientError::from_status(status.as_u16(), &text));
        }
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| ClientError::EmptyBody)?;
        let answers = v["answers"].as_object().ok_or(ClientError::EmptyBody)?;
        Ok(answers
            .iter()
            .filter_map(|(k, a)| Answer::parse(a).map(|a| (k.clone(), a)))
            .collect())
    }
}

/// The state text a verifier judges: task, expectation, and the screen.
pub fn verifier_state(task: &str, expect: &str, state: &str) -> String {
    format!(
        "Task: {task}\nExpected after the last action: {expect}\n\
         Current screen (accessibility elements):\n{state}"
    )
}

/// A [`Verifier`] backed by JEV. Turns itself off after an "unavailable"
/// answer (no key, free tier) so it costs nothing on later steps.
pub struct JevVerifier {
    client: DecisionClient,
    off: AtomicBool,
}

impl JevVerifier {
    pub fn new(client: DecisionClient) -> Self {
        Self {
            client,
            off: AtomicBool::new(false),
        }
    }

    pub fn is_off(&self) -> bool {
        self.off.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl Verifier for JevVerifier {
    async fn check(&self, task: &str, expect: &str, state: &str) -> Option<f32> {
        if self.is_off() || expect.trim().is_empty() {
            return None;
        }
        let q = Question::Noul {
            instructions: format!(
                "Does the current screen show that the expectation is met: {expect}"
            ),
        };
        match self
            .client
            .ask(&verifier_state(task, expect, state), &[("met", q)])
            .await
        {
            Ok(a) => a
                .get("met")
                .and_then(Answer::probability)
                .map(|p| p.clamp(0.0, 1.0) as f32),
            Err(e @ (ClientError::Unavailable(_) | ClientError::MissingKey(_))) => {
                log::warn!("JEV verifier off: {e}");
                self.off.store(true, Ordering::Relaxed);
                None
            }
            Err(e) => {
                log::debug!("JEV verifier skipped this step: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_parse_in_both_shapes() {
        let n = Answer::parse(&serde_json::json!({"type":"noul","noul":0.98})).unwrap();
        assert_eq!(n.probability(), Some(0.98));
        let b = Answer::parse(&serde_json::json!({"type":"boolean","probability":0.2})).unwrap();
        assert_eq!(b.probability(), Some(0.2));
        let b2 = Answer::parse(&serde_json::json!({"type":"boolean","value":true})).unwrap();
        assert_eq!(b2.probability(), Some(1.0));
        let c = Answer::parse(&serde_json::json!({"type":"choice","choice":"save"})).unwrap();
        assert_eq!(
            c,
            Answer::Choice {
                choice: "save".into()
            }
        );
        let s = Answer::parse(&serde_json::json!({"type":"score","score":7.5})).unwrap();
        assert_eq!(s, Answer::Score(7.5));
        assert!(Answer::parse(&serde_json::json!({"type":"other"})).is_none());
    }

    #[test]
    fn noul_is_sent_as_boolean_on_evaluate() {
        let q = [(
            "met",
            Question::Noul {
                instructions: "x?".into(),
            },
        )];
        let so = DecisionClient::gateway("k");
        assert_eq!(so.body("s", &q)["questions"]["met"]["type"], "noul");
        let ev = DecisionClient::gateway("k").with_endpoint(JevEndpoint::Evaluate);
        let b = ev.body("s", &q);
        assert_eq!(b["questions"]["met"]["type"], "boolean");
        assert_eq!(b["model"], "typesafe-ai/jev");
        assert_eq!(b["state"], "s");
    }
}
