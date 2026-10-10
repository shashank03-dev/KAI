//! TypeSafe AI's JEV ("System One") decision model over the AI Gateway: typed
//! questions about a text state, answered with calibrated numbers in one fast,
//! cheap call. Used as a [`Verifier`] for a step's `expect`.
//!
//! Two endpoint shapes exist (both measured to validate requests):
//! - `POST {base}/v1/evaluate`: question types `boolean | choice | score`.
//! - `POST {base}/typesafe/v1/systemone`: also `noul` (a probability).
//! - `POST {base}/v1/systemone`: the same wire format served locally by
//!   jevos-v4 (`./jev serve`, port 8017; auth only if `JEV_API_KEY` is set),
//!   used by the executor (`crate::executor`, `docs/EXECUTOR.md`).
//!
//! On the Gateway free tier both answer 403 "Free tier users do not have
//! access"; that surfaces as [`ClientError::Unavailable`], and the verifier
//! turns itself off and returns `None` so the loop proceeds without it.

pub use super::jev_answer::Answer;
use super::{ClientError, http_client, send_error};
use crate::ports::Verifier;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Where jevos-v4 listens by default (`./jev serve`).
pub const LOCAL_BASE: &str = "http://127.0.0.1:8017";

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
    /// A choice whose options carry descriptions: sent as `"type":"choice"`
    /// with `"criteria": {"<option>": "<description>", ...}` (jevos' format).
    Criteria {
        instructions: String,
        criteria: Vec<(String, String)>,
    },
    /// A score over named levels: `"type":"score"` with `"criteria": [..]`.
    Levels {
        instructions: String,
        levels: Vec<String>,
    },
}

/// Which request shape to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JevEndpoint {
    /// `/v1/evaluate` (no `noul`).
    Evaluate,
    /// `/typesafe/v1/systemone`.
    SystemOne,
    /// `/v1/systemone` on a local jevos server (no `/typesafe` prefix).
    Local,
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

    /// jevos-v4 served locally at `base` (e.g. [`LOCAL_BASE`]). No key unless
    /// [`DecisionClient::with_key`] sets one.
    pub fn local(base: impl Into<String>) -> Self {
        Self {
            http: http_client(),
            base: String::new(),
            api_key: String::new(),
            model: "jev-latest".to_string(),
            endpoint: JevEndpoint::Local,
            timeout: Duration::from_secs(3),
        }
        .with_base(base)
    }

    /// Bearer key (empty: no `Authorization` header on the local endpoint).
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = key.into();
        self
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn endpoint(&self) -> JevEndpoint {
        self.endpoint
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

    pub(crate) fn body(
        &self,
        state: serde_json::Value,
        questions: &[(&str, Question)],
    ) -> serde_json::Value {
        let mut qs = serde_json::Map::new();
        for (id, q) in questions {
            let v = match q {
                Question::Noul { instructions } => {
                    let ty = match self.endpoint {
                        JevEndpoint::SystemOne | JevEndpoint::Local => "noul",
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
                } if self.endpoint == JevEndpoint::Local => {
                    let c: serde_json::Map<_, _> = options
                        .iter()
                        .map(|o| (o.clone(), o.as_str().into()))
                        .collect();
                    serde_json::json!({
                        "type": "choice", "instructions": instructions, "criteria": c
                    })
                }
                Question::Choice {
                    instructions,
                    options,
                } => serde_json::json!({
                    "type": "choice", "instructions": instructions, "options": options
                }),
                Question::Criteria {
                    instructions,
                    criteria,
                } => {
                    let c: serde_json::Map<_, _> = criteria
                        .iter()
                        .map(|(o, d)| (o.clone(), d.as_str().into()))
                        .collect();
                    serde_json::json!({
                        "type": "choice", "instructions": instructions, "criteria": c
                    })
                }
                Question::Levels {
                    instructions,
                    levels,
                } => serde_json::json!({
                    "type": "score", "instructions": instructions, "criteria": levels
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
        self.ask_value(state.into(), questions).await
    }

    /// [`DecisionClient::ask`] with the state as a string or a JSON object.
    pub async fn ask_value(
        &self,
        state: serde_json::Value,
        questions: &[(&str, Question)],
    ) -> Result<BTreeMap<String, Answer>, ClientError> {
        if self.api_key.is_empty() && self.endpoint != JevEndpoint::Local {
            return Err(ClientError::MissingKey("AI_GATEWAY_API_KEY"));
        }
        let path = match self.endpoint {
            JevEndpoint::Evaluate => "/v1/evaluate",
            JevEndpoint::SystemOne => "/typesafe/v1/systemone",
            JevEndpoint::Local => "/v1/systemone",
        };
        let mut req = self
            .http
            .post(format!("{}{path}", self.base))
            .json(&self.body(state, questions))
            .timeout(self.timeout);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req.send().await.map_err(|e| send_error(e, self.timeout))?;
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

    /// `GET {base}/health`: the round-trip time when the server answers 2xx.
    pub async fn health(&self) -> Result<Duration, ClientError> {
        let t = Instant::now();
        let timeout = self.timeout.min(Duration::from_secs(2));
        let mut req = self
            .http
            .get(format!("{}/health", self.base))
            .timeout(timeout);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req.send().await.map_err(|e| send_error(e, timeout))?;
        let status = resp.status();
        let _ = resp.bytes().await;
        if !status.is_success() {
            return Err(ClientError::Status(status.as_u16(), "health check".into()));
        }
        Ok(t.elapsed())
    }
}

#[async_trait]
impl crate::executor::Decider for DecisionClient {
    async fn ask(
        &self,
        state: &serde_json::Value,
        questions: &[(String, Question)],
    ) -> Result<BTreeMap<String, Answer>, ClientError> {
        let qs: Vec<(&str, Question)> = questions
            .iter()
            .map(|(id, q)| (id.as_str(), q.clone()))
            .collect();
        self.ask_value(state.clone(), &qs).await
    }

    async fn health(&self) -> Result<Duration, ClientError> {
        DecisionClient::health(self).await
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
    fn noul_is_sent_as_boolean_on_evaluate() {
        let q = [(
            "met",
            Question::Noul {
                instructions: "x?".into(),
            },
        )];
        let so = DecisionClient::gateway("k");
        assert_eq!(so.body("s".into(), &q)["questions"]["met"]["type"], "noul");
        let ev = DecisionClient::gateway("k").with_endpoint(JevEndpoint::Evaluate);
        let b = ev.body("s".into(), &q);
        assert_eq!(b["questions"]["met"]["type"], "boolean");
        assert_eq!(b["model"], "typesafe-ai/jev");
        assert_eq!(b["state"], "s");
    }
}
