//! Record model requests and aggregate usage for benchmark summaries.
use async_trait::async_trait;
use kai_core::ModelClient;
use kai_core::client::{Backend, ClientError, ModelReply, ModelRequest};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// One model call as seen from outside the client.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
pub struct Call {
    pub ms: u64,
    pub ok: bool,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

/// Wraps a client and records every call into a shared log.
pub struct Metered<'a> {
    inner: &'a dyn ModelClient,
    log: Arc<Mutex<Vec<Call>>>,
}

impl<'a> Metered<'a> {
    pub fn new(inner: &'a dyn ModelClient, log: Arc<Mutex<Vec<Call>>>) -> Self {
        Self { inner, log }
    }
}

#[async_trait]
impl ModelClient for Metered<'_> {
    fn backend(&self) -> Backend {
        self.inner.backend()
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        let started = Instant::now();
        let r = self.inner.decide(req).await;
        let call = match &r {
            Ok(reply) => Call {
                ms: reply.total.as_millis() as u64,
                ok: true,
                prompt_tokens: reply.prompt_tokens,
                completion_tokens: reply.completion_tokens,
            },
            Err(_) => Call {
                ms: started.elapsed().as_millis() as u64,
                ..Default::default()
            },
        };
        self.log.lock().unwrap().push(call);
        r
    }

    async fn sample(
        &self,
        req: &ModelRequest<'_>,
        n: usize,
    ) -> Vec<Result<ModelReply, ClientError>> {
        let started = Instant::now();
        let initial_backend = self.inner.backend();
        let replies = self.inner.sample(req, n).await;
        let final_backend = self.inner.backend();
        let mut log = self.log.lock().unwrap();
        let batched_vllm = final_backend == Backend::Vllm
            && !replies
                .iter()
                .skip(1)
                .any(|r| r.as_ref().is_ok_and(|reply| !reply.total.is_zero()));
        if batched_vllm {
            // A single n=K vLLM completion reports request-level usage only
            // on its first choice; all later choice durations are zero.
            log.push(Call {
                ms: started.elapsed().as_millis() as u64,
                ok: replies.iter().any(Result::is_ok),
                prompt_tokens: replies
                    .iter()
                    .filter_map(|r| r.as_ref().ok().and_then(|v| v.prompt_tokens))
                    .sum::<u32>()
                    .into(),
                completion_tokens: replies
                    .iter()
                    .filter_map(|r| r.as_ref().ok().and_then(|v| v.completion_tokens))
                    .sum::<u32>()
                    .into(),
            });
        } else {
            // vLLM may fall back to independent requests on endpoints that
            // reject n=K. Count the failed batch attempt as well.
            if initial_backend == Backend::Vllm {
                log.push(Call {
                    ms: started.elapsed().as_millis() as u64,
                    ..Default::default()
                });
            }
            log.extend(replies.iter().map(|r| match r {
                Ok(reply) => Call {
                    ms: reply.total.as_millis() as u64,
                    ok: true,
                    prompt_tokens: reply.prompt_tokens,
                    completion_tokens: reply.completion_tokens,
                },
                Err(_) => Call {
                    ms: started.elapsed().as_millis() as u64,
                    ..Default::default()
                },
            }));
        }
        replies
    }
}
