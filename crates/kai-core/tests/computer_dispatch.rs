use async_trait::async_trait;
use kai_core::Rect;
use kai_core::computer::*;
use kai_core::ports::NeverStops;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Driver {
    calls: AtomicUsize,
    stale: bool,
}
#[async_trait]
impl ComputerExecutor for Driver {
    fn capabilities(&self) -> Capabilities {
        Capabilities::all()
    }
    async fn is_current(
        &self,
        _: &ComputerAction,
        _: &ObservationSpace,
    ) -> Result<bool, ToolError> {
        Ok(!self.stale)
    }
    async fn execute(
        &self,
        _: &ComputerAction,
        _: &ObservationSpace,
        ctx: &ExecutionContext,
    ) -> Result<(), ToolError> {
        ctx.check()?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
struct Deny;
#[async_trait]
impl ComputerPolicy for Deny {
    async fn authorize(&self, _: &ToolRequest) -> Result<(), ToolError> {
        Err(ToolError::PermissionDenied)
    }
}
struct Permit;
#[async_trait]
impl ComputerPolicy for Permit {
    async fn authorize(&self, _: &ToolRequest) -> Result<(), ToolError> {
        Ok(())
    }
}
fn space() -> ObservationSpace {
    ObservationSpace {
        session: 1,
        id: 1,
        image_width: 100,
        image_height: 100,
        region: Rect::new(0., 0., 100., 100.),
    }
}
fn request(id: u64) -> ToolRequest {
    ToolRequest {
        version: 1,
        id,
        session: 1,
        timeout_ms: 100,
        action: ComputerAction::Move {
            target: Target::Point {
                observation: 1,
                x: 10.,
                y: 10.,
            },
            modifiers: vec![],
        },
    }
}

#[tokio::test]
async fn denied_stale_and_replayed_requests_cannot_mutate() {
    let d = Driver {
        calls: AtomicUsize::new(0),
        stale: false,
    };
    let deny = Dispatcher::new(&d, &Deny, &NeverStops);
    assert_eq!(
        deny.dispatch(&request(1), &space()).await.error,
        Some(ToolError::PermissionDenied)
    );
    assert_eq!(d.calls.load(Ordering::SeqCst), 0);
    let allow = Dispatcher::new(&d, &Permit, &NeverStops);
    assert_eq!(allow.dispatch(&request(1), &space()).await.error, None);
    assert_eq!(
        allow.dispatch(&request(1), &space()).await.error,
        Some(ToolError::InvalidArguments)
    );
    assert_eq!(d.calls.load(Ordering::SeqCst), 1);
    let stale = Driver {
        calls: AtomicUsize::new(0),
        stale: true,
    };
    assert_eq!(
        Dispatcher::new(&stale, &Permit, &NeverStops)
            .dispatch(&request(1), &space())
            .await
            .error,
        Some(ToolError::StaleObservation)
    );
    assert_eq!(stale.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn malformed_batch_is_rejected_before_any_mutation() {
    let d = Driver {
        calls: AtomicUsize::new(0),
        stale: false,
    };
    let mut bad = request(2);
    bad.version = 99;
    let results = Dispatcher::new(&d, &Permit, &NeverStops)
        .batch(&[request(1), bad], &space())
        .await;
    assert!(
        results
            .iter()
            .all(|r| r.error == Some(ToolError::InvalidArguments))
    );
    assert_eq!(d.calls.load(Ordering::SeqCst), 0);
}

struct CancelOnPermission<'a>(&'a Stop);
struct Stop(std::sync::atomic::AtomicBool);
impl kai_core::Interrupt for Stop {
    fn stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
#[async_trait]
impl ComputerPolicy for CancelOnPermission<'_> {
    async fn authorize(&self, _: &ToolRequest) -> Result<(), ToolError> {
        self.0.0.store(true, Ordering::SeqCst);
        Ok(())
    }
}
#[tokio::test]
async fn cancellation_after_permission_still_prevents_input() {
    let d = Driver {
        calls: AtomicUsize::new(0),
        stale: false,
    };
    let stop = Stop(std::sync::atomic::AtomicBool::new(false));
    let policy = CancelOnPermission(&stop);
    let result = Dispatcher::new(&d, &policy, &stop)
        .dispatch(&request(1), &space())
        .await;
    assert_eq!(result.error, Some(ToolError::Cancelled));
    assert_eq!(d.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn dropping_execution_owner_invalidates_queued_driver_context() {
    let ctx = ExecutionContext::new(std::time::Duration::from_secs(1));
    let queued = ctx.clone();
    {
        let _owner = ExecutionGuard(ctx);
        assert!(queued.check().is_ok());
    }
    assert_eq!(queued.check(), Err(ToolError::Cancelled));
}

struct Exclusive {
    active: AtomicUsize,
    peak: AtomicUsize,
}
#[async_trait]
impl ComputerExecutor for Exclusive {
    fn capabilities(&self) -> Capabilities {
        Capabilities::all()
    }
    async fn is_current(
        &self,
        _: &ComputerAction,
        _: &ObservationSpace,
    ) -> Result<bool, ToolError> {
        Ok(true)
    }
    async fn execute(
        &self,
        _: &ComputerAction,
        _: &ObservationSpace,
        _: &ExecutionContext,
    ) -> Result<(), ToolError> {
        let n = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(n, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}
#[tokio::test]
async fn separate_dispatchers_share_one_input_owner() {
    let driver = Exclusive {
        active: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
    };
    let a = Dispatcher::new(&driver, &Permit, &NeverStops);
    let b = Dispatcher::new(&driver, &Permit, &NeverStops);
    let request = request(1);
    let space = space();
    let (a, b) = tokio::join!(a.dispatch(&request, &space), b.dispatch(&request, &space));
    assert_eq!(a.error, None);
    assert_eq!(b.error, None);
    assert_eq!(driver.peak.load(Ordering::SeqCst), 1);
}
