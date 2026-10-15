//! Every executor shares validation, authorization, cancellation and deadlines.
//! An executor is trusted code; a model can only supply serialized requests.

use super::*;
use crate::Interrupt;
use async_trait::async_trait;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub struct ExecutionGuard(pub ExecutionContext);
impl Drop for ExecutionGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Clone)]
pub struct ExecutionContext {
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}

impl ExecutionContext {
    pub fn new(budget: Duration) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + budget,
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
    pub fn check(&self) -> Result<(), ToolError> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err(ToolError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(ToolError::TimedOut)
        } else {
            Ok(())
        }
    }
}

#[async_trait]
pub trait ComputerExecutor: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    /// Revalidate captured geometry/state immediately before side effects.
    async fn is_current(
        &self,
        action: &ComputerAction,
        space: &ObservationSpace,
    ) -> Result<bool, ToolError>;
    async fn execute(
        &self,
        action: &ComputerAction,
        space: &ObservationSpace,
        context: &ExecutionContext,
    ) -> Result<(), ToolError>;
}

#[async_trait]
pub trait ComputerPolicy: Send + Sync {
    /// The host must authorize this exact request. No default allow policy.
    async fn authorize(&self, request: &ToolRequest) -> Result<(), ToolError>;
}

/// One mutation owner per session. Request IDs increase strictly, including
/// failed requests; replay requires a new request and another authorization.
static MUTATION_OWNER: Mutex<()> = Mutex::const_new(());
/// Host input ownership; independent terminal jobs need not take this lock.
pub async fn mutation_owner() -> tokio::sync::MutexGuard<'static, ()> {
    MUTATION_OWNER.lock().await
}

pub struct Dispatcher<'a> {
    executor: &'a dyn ComputerExecutor,
    policy: &'a dyn ComputerPolicy,
    interrupt: &'a dyn Interrupt,
    owner: Mutex<Option<(u64, u64)>>,
}

impl<'a> Dispatcher<'a> {
    pub fn new(
        executor: &'a dyn ComputerExecutor,
        policy: &'a dyn ComputerPolicy,
        interrupt: &'a dyn Interrupt,
    ) -> Self {
        Self {
            executor,
            policy,
            interrupt,
            owner: Mutex::new(None),
        }
    }

    pub async fn dispatch(&self, request: &ToolRequest, space: &ObservationSpace) -> ToolResult {
        let started = Instant::now();
        let ctx = ExecutionContext::new(Duration::from_millis(request.timeout_ms as u64));
        let _guard = ExecutionGuard(ctx.clone());
        let operation = async {
            let _mutation = MUTATION_OWNER.lock().await;
            request.validate(&self.executor.capabilities(), space)?;
            // This lock is also held during authorization: another request
            // cannot change the desktop while the person reviews the action.
            let mut owner = self.owner.lock().await;
            if let Some((session, last)) = *owner {
                if session != request.session {
                    return Err(ToolError::StaleObservation);
                }
                if request.id <= last {
                    return Err(ToolError::InvalidArguments);
                }
            }
            *owner = Some((request.session, request.id));
            self.policy.authorize(request).await?;
            self.check(&ctx)?;
            if !self.executor.is_current(&request.action, space).await? {
                return Err(ToolError::StaleObservation);
            }
            self.check(&ctx)?;
            self.executor.execute(&request.action, space, &ctx).await
        };
        let result = tokio::select! {
            biased;
            () = crate::ports::wait_cancelled(self.interrupt) => Err(ToolError::Cancelled),
            () = tokio::time::sleep(Duration::from_millis(request.timeout_ms as u64)) => Err(ToolError::TimedOut),
            result = operation => result,
        };
        // Queued driver work holds a clone, so dropping the future does not
        // permit it to execute after cancellation/timeout has been reported.
        ctx.cancel();
        ToolResult {
            id: request.id,
            session: request.session,
            error: result.err(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            requires_observation: !matches!(
                request.action,
                ComputerAction::Observe | ComputerAction::Wait { .. }
            ),
        }
    }

    fn check(&self, ctx: &ExecutionContext) -> Result<(), ToolError> {
        if self.interrupt.cancelled() {
            ctx.cancel();
        }
        ctx.check()
    }

    pub async fn batch(
        &self,
        requests: &[ToolRequest],
        space: &ObservationSpace,
    ) -> Vec<ToolResult> {
        if let Err(error) = validate_batch(requests, &self.executor.capabilities(), space) {
            return requests
                .iter()
                .map(|r| ToolResult {
                    id: r.id,
                    session: r.session,
                    error: Some(error),
                    elapsed_ms: 0,
                    requires_observation: false,
                })
                .collect();
        }
        let mut results = Vec::new();
        for request in requests {
            let result = self.dispatch(request, space).await;
            let failed = result.error.is_some();
            results.push(result);
            if failed {
                break;
            }
        }
        results
    }
}
