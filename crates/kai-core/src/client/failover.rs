//! Automatic fallback: prefer the Droplet (vLLM); on a failover-class error
//! switch to Gemini for this step and keep using it for a cooldown, then try
//! the Droplet again.

use super::{Backend, ClientError, ModelClient, ModelReply, ModelRequest};
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Wraps a primary and a secondary client.
pub struct FailoverClient {
    primary: Box<dyn ModelClient>,
    secondary: Option<Box<dyn ModelClient>>,
    cooldown: Duration,
    /// When set, stay on the secondary until this instant.
    until: Mutex<Option<Instant>>,
}

impl FailoverClient {
    pub fn new(primary: Box<dyn ModelClient>, secondary: Option<Box<dyn ModelClient>>) -> Self {
        Self {
            primary,
            secondary,
            cooldown: Duration::from_secs(30),
            until: Mutex::new(None),
        }
    }

    pub fn with_cooldown(mut self, d: Duration) -> Self {
        self.cooldown = d;
        self
    }

    fn in_cooldown(&self) -> bool {
        self.until
            .lock()
            .unwrap()
            .map(|t| Instant::now() < t)
            .unwrap_or(false)
    }

    fn start_cooldown(&self) {
        *self.until.lock().unwrap() = Some(Instant::now() + self.cooldown);
    }

    fn clear_cooldown(&self) {
        *self.until.lock().unwrap() = None;
    }
}

#[async_trait]
impl ModelClient for FailoverClient {
    fn backend(&self) -> Backend {
        if self.in_cooldown() {
            self.secondary
                .as_ref()
                .map(|s| s.backend())
                .unwrap_or_else(|| self.primary.backend())
        } else {
            self.primary.backend()
        }
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        // While cooling down, go straight to the secondary.
        if self.in_cooldown()
            && let Some(sec) = &self.secondary
        {
            return sec.decide(req).await;
        }

        match self.primary.decide(req).await {
            Ok(r) => {
                self.clear_cooldown();
                Ok(r)
            }
            Err(e) if e.is_failover() && self.secondary.is_some() => {
                log::warn!(
                    "primary backend failing over to secondary ({e}); cooldown {:?}",
                    self.cooldown
                );
                if matches!(e, ClientError::Unavailable(_)) {
                    // Not a blip: this account cannot use the primary model.
                    // Stay on the secondary for the rest of the process.
                    *self.until.lock().unwrap() =
                        Some(Instant::now() + Duration::from_secs(365 * 24 * 3600));
                } else {
                    self.start_cooldown();
                }
                self.secondary.as_ref().unwrap().decide(req).await
            }
            Err(e) => Err(e),
        }
    }

    async fn sample(
        &self,
        req: &ModelRequest<'_>,
        n: usize,
    ) -> Vec<Result<ModelReply, ClientError>> {
        if self.in_cooldown()
            && let Some(secondary) = &self.secondary
        {
            return secondary.sample(req, n).await;
        }
        // Preserve a vLLM primary's single n=K HTTP request. The wrapper's
        // default trait method would instead make K independent decide calls.
        let replies = self.primary.sample(req, n).await;
        let failover = !replies.is_empty()
            && replies
                .iter()
                .all(|r| r.as_ref().is_err_and(ClientError::is_failover));
        if failover && let Some(secondary) = &self.secondary {
            log::warn!("candidate sampling failed over to secondary");
            if replies
                .iter()
                .any(|r| matches!(r, Err(ClientError::Unavailable(_))))
            {
                *self.until.lock().unwrap() =
                    Some(Instant::now() + Duration::from_secs(365 * 24 * 3600));
            } else {
                self.start_cooldown();
            }
            return secondary.sample(req, n).await;
        }
        if replies.iter().any(Result::is_ok) {
            self.clear_cooldown();
        }
        replies
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{Images, MockClient};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn rate_limit_is_a_failover_error() {
        assert!(ClientError::from_status(429, "busy").is_failover());
    }

    struct AlwaysFails;
    #[async_trait]
    impl ModelClient for AlwaysFails {
        fn backend(&self) -> Backend {
            Backend::Vllm
        }
        async fn decide(&self, _: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
            Err(ClientError::Timeout(Duration::from_secs(1)))
        }
    }

    fn req<'a>(schema: &'a serde_json::Value, imgs: &'a Images) -> ModelRequest<'a> {
        ModelRequest {
            observation_id: None,
            computer_result: None,
            system: "s",
            user_text: "u",
            images: imgs,
            schema,
            max_tokens: 1,
            temperature: 0.0,
        }
    }

    #[tokio::test]
    async fn fails_over_and_stays_on_secondary_during_cooldown() {
        let f = FailoverClient::new(
            Box::new(AlwaysFails),
            Some(Box::new(MockClient::always("secondary"))),
        );
        let schema = crate::schema::Decision::json_schema();
        let imgs = Images {
            screenshot_jpeg: vec![],
            crop_jpeg: None,
        };
        // First call fails over.
        let r = f.decide(&req(&schema, &imgs)).await.unwrap();
        assert_eq!(r.raw, "secondary");
        assert_eq!(r.backend, Backend::Mock);
        // During cooldown we don't even touch the (failing) primary.
        assert_eq!(f.backend(), Backend::Mock);
        let r2 = f.decide(&req(&schema, &imgs)).await.unwrap();
        assert_eq!(r2.raw, "secondary");
    }

    struct Restricted;
    #[async_trait]
    impl ModelClient for Restricted {
        fn backend(&self) -> Backend {
            Backend::OpenAi
        }
        async fn decide(&self, _: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
            Err(ClientError::from_status(
                403,
                r#"{"error":{"message":"Free tier users do not have access to this model."}}"#,
            ))
        }
    }

    struct RateLimited;
    #[async_trait]
    impl ModelClient for RateLimited {
        fn backend(&self) -> Backend {
            Backend::Vllm
        }
        async fn decide(&self, _: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
            Err(ClientError::from_status(429, "busy"))
        }
    }

    #[tokio::test]
    async fn rate_limit_fails_over() {
        let f = FailoverClient::new(
            Box::new(RateLimited),
            Some(Box::new(MockClient::always("secondary"))),
        );
        let schema = crate::schema::Decision::json_schema();
        let imgs = Images {
            screenshot_jpeg: vec![],
            crop_jpeg: None,
        };
        assert_eq!(
            f.decide(&req(&schema, &imgs)).await.unwrap().raw,
            "secondary"
        );
    }

    #[tokio::test]
    async fn unavailable_model_fails_over_for_good() {
        let f = FailoverClient::new(
            Box::new(Restricted),
            Some(Box::new(MockClient::always("fast"))),
        )
        .with_cooldown(Duration::ZERO);
        let schema = crate::schema::Decision::json_schema();
        let imgs = Images {
            screenshot_jpeg: vec![],
            crop_jpeg: None,
        };
        assert_eq!(f.decide(&req(&schema, &imgs)).await.unwrap().raw, "fast");
        // Even with a zero cooldown the restricted primary is not retried.
        assert_eq!(f.backend(), Backend::Mock);
    }

    #[tokio::test]
    async fn no_secondary_propagates_error() {
        let f = FailoverClient::new(Box::new(AlwaysFails), None);
        let schema = crate::schema::Decision::json_schema();
        let imgs = Images {
            screenshot_jpeg: vec![],
            crop_jpeg: None,
        };
        assert!(f.decide(&req(&schema, &imgs)).await.is_err());
    }

    struct SamplesOnly(Arc<AtomicUsize>);
    #[async_trait]
    impl ModelClient for SamplesOnly {
        fn backend(&self) -> Backend {
            Backend::Vllm
        }
        async fn decide(&self, _: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
            panic!("batched sampling must not call decide")
        }
        async fn sample(
            &self,
            _: &ModelRequest<'_>,
            n: usize,
        ) -> Vec<Result<ModelReply, ClientError>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            (0..n).map(|_| Err(ClientError::EmptyBody)).collect()
        }
    }

    #[tokio::test]
    async fn sampling_preserves_primary_multi_choice_call() {
        let seen = Arc::new(AtomicUsize::new(0));
        let client = FailoverClient::new(Box::new(SamplesOnly(seen.clone())), None);
        let schema = crate::schema::Decision::json_schema();
        let imgs = Images {
            screenshot_jpeg: vec![],
            crop_jpeg: None,
        };
        assert_eq!(client.sample(&req(&schema, &imgs), 4).await.len(), 4);
        assert_eq!(seen.load(Ordering::SeqCst), 1);
    }
}
