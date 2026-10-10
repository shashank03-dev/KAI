//! Model backends behind one trait. The harness talks only to [`ModelClient`];
//! swapping vLLM for Gemini for a mock changes nothing above this line.

mod failover;
mod gemini;
pub mod jev;
mod jev_answer;
mod llm_verifier;
mod mock;
mod openai;
pub mod stream;
mod vllm;

pub use failover::FailoverClient;
pub use gemini::GeminiClient;
pub use jev::{Answer, DecisionClient, JevVerifier, Question};
pub use llm_verifier::LlmVerifier;
pub use mock::MockClient;
pub use openai::{GATEWAY_BASE, OPENAI_BASE, OpenAiClient, gateway_model_id};
pub use vllm::VllmClient;

use async_trait::async_trait;
use std::time::Duration;

/// One screenshot plus an optional crop, already JPEG-encoded.
#[derive(Clone)]
pub struct Images {
    pub screenshot_jpeg: Vec<u8>,
    pub crop_jpeg: Option<Vec<u8>>,
}

/// Everything a single `decide` call needs. Laid out stable-first by
/// [`crate::prompt`] so vLLM prefix caching hits.
pub struct ModelRequest<'a> {
    /// Stable system prompt (cached prefix).
    pub system: &'a str,
    /// The changing user content: element list + last-3 history + task.
    pub user_text: &'a str,
    pub images: &'a Images,
    /// JSON schema for structured outputs (vLLM); Gemini puts it in the prompt.
    pub schema: &'a serde_json::Value,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// What came back, with timings for the trace.
#[derive(Debug, Clone)]
pub struct ModelReply {
    pub raw: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    /// Time to first token, when the backend was called in streaming mode.
    pub first_token: Option<Duration>,
    /// Wall time for the whole call.
    pub total: Duration,
    /// Which backend actually served this (for the status bar).
    pub backend: Backend,
    /// What the call cost in US$, when the backend reports it (AI Gateway).
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Vllm,
    Gemini,
    /// Any OpenAI-compatible endpoint: Vercel AI Gateway or OpenAI itself.
    OpenAi,
    /// OpenAI's own computer-use loop (native, not Kai's harness).
    OpenAiNative,
    Mock,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Vllm => "DigitalOcean (Gemma 4)",
            Backend::Gemini => "Gemini API (Gemma 4)",
            Backend::OpenAi => "OpenAI-compatible (Gateway/OpenAI)",
            Backend::OpenAiNative => "OpenAI computer use (native loop)",
            Backend::Mock => "Mock",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("network/transport error: {0}")]
    Transport(String),
    #[error("backend returned status {0}: {1}")]
    Status(u16, String),
    #[error("backend timed out after {0:?}")]
    Timeout(Duration),
    #[error("empty or malformed response body")]
    EmptyBody,
    /// The model exists but this account may not use it (e.g. the AI
    /// Gateway's free tier answers 403 for larger models). Not retryable.
    #[error("model not available on this account: {0}")]
    Unavailable(String),
    /// No API key configured for this backend.
    #[error("missing API key: {0}")]
    MissingKey(&'static str),
}

impl ClientError {
    /// Whether the fallback should kick in for this error (Droplet unreachable
    /// or slow → try Gemini; model not available on this tier → other model).
    pub fn is_failover(&self) -> bool {
        matches!(
            self,
            ClientError::Transport(_)
                | ClientError::Timeout(_)
                | ClientError::Status(500..=599, _)
                | ClientError::Unavailable(_)
        )
    }

    /// Map an HTTP error status and body to an error. A 401/403/404 that says
    /// the account cannot use the model becomes [`ClientError::Unavailable`].
    pub fn from_status(status: u16, body: &str) -> Self {
        let lower = body.to_ascii_lowercase();
        let restricted = lower.contains("free tier")
            || lower.contains("restrictedmodels")
            || lower.contains("no_providers_available")
            || lower.contains("model_not_found")
            || lower.contains("does not have access");
        let short: String = body.chars().take(300).collect();
        if matches!(status, 401 | 403 | 404) && restricted {
            ClientError::Unavailable(short)
        } else {
            ClientError::Status(status, short)
        }
    }
}

/// A shared HTTP client: keep-alive pool, HTTP/2 when offered, the system
/// proxy, and extra trusted roots from `SSL_CERT_FILE` (corporate proxies).
pub fn http_client() -> reqwest::Client {
    let mut b = reqwest::Client::builder()
        .pool_max_idle_per_host(4)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30))
        .tcp_nodelay(true);
    if let Some(path) = std::env::var_os("SSL_CERT_FILE")
        && let Ok(pem) = std::fs::read(&path)
        && let Ok(certs) = reqwest::Certificate::from_pem_bundle(&pem)
    {
        for c in certs {
            b = b.add_root_certificate(c);
        }
    }
    b.build().expect("reqwest client")
}

/// Map a reqwest send error to a [`ClientError`].
pub(crate) fn send_error(e: reqwest::Error, timeout: Duration) -> ClientError {
    if e.is_timeout() {
        ClientError::Timeout(timeout)
    } else {
        ClientError::Transport(e.to_string())
    }
}

/// A model that can choose the next action from a screenshot + element list.
#[async_trait]
pub trait ModelClient: Send + Sync {
    fn backend(&self) -> Backend;

    /// One decision call. Returns the raw reply; the caller validates it.
    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError>;

    /// A cheap warm-up (used when the app opens / onboarding "Test" button).
    async fn warm_up(&self, images: &Images) -> Result<ModelReply, ClientError> {
        let schema = crate::schema::Decision::json_schema();
        let req = ModelRequest {
            system: "You are Kai. Reply with a `done` action.",
            user_text: "warm up",
            images,
            schema: &schema,
            max_tokens: 16,
            temperature: 0.0,
        };
        self.decide(&req).await
    }
}
