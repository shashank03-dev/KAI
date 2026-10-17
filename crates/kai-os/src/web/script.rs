//! Capability-limited, persistent JavaScript for browser procedures.
//!
//! JavaScript runs in QuickJS with no host modules or bindings to filesystem,
//! network, CDP, processes, or the page's JS realm. It can only enqueue typed
//! browser operations. Each operation is resolved against the live browser and
//! goes through the shared computer dispatcher and a REQUIRED host policy.
//! This is an experimental restricted browser-code facility, not Playwright.

use super::lane::BrowserLane;
use kai_core::computer::{
    ActionKind, Capabilities, ComputerAction, ComputerExecutor, ComputerPolicy, Dispatcher,
    ExecutionContext, ExecutionGuard, MouseButton, ObservationSpace, Target, ToolError,
    ToolRequest, ToolResult,
};
use kai_core::{Actuator, Grounding, Interrupt, Rect};
use rquickjs::{Context, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const MAX_SOURCE: usize = 16 * 1024;
const MAX_ACTIONS: usize = 32;
const MAX_OUTPUT: usize = 16 * 1024;
const HEAP_LIMIT: usize = 8 * 1024 * 1024;
static NEXT_SCRIPT_SESSION: AtomicU64 = AtomicU64::new(1);
const BOOTSTRAP: &str = r#"
(() => {
  const actions = [], logs = [];
  const emit = (kind, data) => { actions.push({kind, ...data}); };
  Object.defineProperty(globalThis, 'kai', {configurable:false, writable:false, value:Object.freeze({
    click: selector => emit('click', {selector}),
    type: text => emit('type', {text}),
    press: keys => emit('press', {keys}),
    open: url => emit('open', {url}),
    wait: duration_ms => emit('wait', {duration_ms}),
    log: (...parts) => { if(logs.length < 32) logs.push(parts.map(String).join(' ').slice(0,1024)); }
  })});
  Object.defineProperty(globalThis, '__kaiDrain', {configurable:false, writable:false,
    value: () => JSON.stringify({actions: actions.splice(0), logs: logs.splice(0)})});
  Object.defineProperty(globalThis, '__kaiDiscard', {configurable:false, writable:false,
    value: () => {actions.length=0; logs.length=0;}});
})();
"#;

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("script limit exceeded: {0}")]
    Limit(&'static str),
    #[error("JavaScript execution failed: {0}")]
    JavaScript(String),
    #[error("script worker is unavailable")]
    Worker,
    #[error("browser selector is missing, obscured, or outside the viewport: {0}")]
    Selector(String),
    #[error("computer action failed: {0}")]
    Computer(#[from] ToolError),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScriptAction {
    Click { selector: String },
    Type { text: String },
    Press { keys: Vec<String> },
    Open { url: String },
    Wait { duration_ms: u32 },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptPlan {
    pub actions: Vec<ScriptAction>,
    pub logs: Vec<String>,
}

impl ScriptPlan {
    fn validate(&self) -> Result<(), ScriptError> {
        if self.actions.len() > MAX_ACTIONS {
            return Err(ScriptError::Limit("too many browser actions"));
        }
        if self.logs.iter().map(String::len).sum::<usize>() > MAX_OUTPUT {
            return Err(ScriptError::Limit("log output"));
        }
        for action in &self.actions {
            match action {
                ScriptAction::Click { selector } if selector.is_empty() || selector.len() > 512 => {
                    return Err(ScriptError::Limit("selector length"));
                }
                ScriptAction::Type { text } if text.len() > 4096 => {
                    return Err(ScriptError::Limit("typed text length"));
                }
                ScriptAction::Press { keys } if keys.is_empty() || keys.len() > 8 => {
                    return Err(ScriptError::Limit("key count"));
                }
                ScriptAction::Open { url } if url.len() > 2048 => {
                    return Err(ScriptError::Limit("URL length"));
                }
                ScriptAction::Wait { duration_ms } if *duration_ms > 3000 => {
                    return Err(ScriptError::Limit("wait duration"));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

struct Work {
    source: String,
    context: ExecutionContext,
    reply: mpsc::SyncSender<Result<ScriptPlan, ScriptError>>,
}

/// Owns a dedicated JS thread; lexical bindings and helper functions persist
/// between calls, and disappear when the session is dropped.
#[derive(Clone)]
pub struct BrowserScript {
    tx: mpsc::SyncSender<Work>,
    serial: Arc<tokio::sync::Mutex<()>>,
    session: u64,
    next_request_id: Arc<AtomicU64>,
}

#[derive(Debug)]
pub struct ScriptOutcome {
    pub logs: Vec<String>,
    pub results: Vec<ToolResult>,
}

impl BrowserScript {
    pub fn new() -> Result<Self, ScriptError> {
        let (tx, rx) = mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("kai-browser-script".into())
            .spawn(move || worker(rx, ready_tx))
            .map_err(|_| ScriptError::Worker)?;
        ready_rx.recv().map_err(|_| ScriptError::Worker)??;
        Ok(Self {
            tx,
            serial: Arc::new(tokio::sync::Mutex::new(())),
            session: NEXT_SCRIPT_SESSION.fetch_add(1, Ordering::Relaxed),
            next_request_id: Arc::new(AtomicU64::new(1)),
        })
    }

    /// Run a persistent JS procedure, then authorize and execute each emitted
    /// action in order. Never supply an implicit allow-all policy in production.
    pub async fn run(
        &self,
        browser: &BrowserLane,
        source: &str,
        policy: &dyn ComputerPolicy,
        interrupt: &dyn Interrupt,
        budget: Duration,
    ) -> Result<ScriptOutcome, ScriptError> {
        let _serial = self.serial.lock().await;
        if source.len() > MAX_SOURCE {
            return Err(ScriptError::Limit("source length"));
        }
        let start = Instant::now();
        let context = ExecutionContext::new(budget);
        let _guard = ExecutionGuard(context.clone());
        let (reply, receiver) = mpsc::sync_channel(1);
        let work = Work {
            source: source.into(),
            context: context.clone(),
            reply,
        };
        // All JS state stays on the dedicated thread. The thread is bounded
        // by QuickJS's instruction interrupt and its heap/stack limits.
        let tx = self.tx.clone();
        let planning = tokio::task::spawn_blocking(move || {
            tx.send(work).map_err(|_| ScriptError::Worker)?;
            receiver.recv().map_err(|_| ScriptError::Worker)?
        });
        let plan = tokio::select! {
            biased;
            () = kai_core::ports::wait_cancelled(interrupt) => {
                context.cancel();
                return Err(ScriptError::Computer(ToolError::Cancelled));
            }
            planned = planning => planned.map_err(|_| ScriptError::Worker)??,
        };
        context.check()?;
        let executor = BrowserExecutor {
            browser,
            selector: Mutex::new(None),
            page_state: Mutex::new(None),
        };
        let dispatcher = Dispatcher::new(&executor, policy, interrupt);
        let mut results = Vec::new();
        // A fresh selector query is made for each step, after the previous
        // interaction has changed the page; the dispatcher checks it again
        // under the mutation lock after the host approves the request.
        for action in plan.actions {
            context.check()?;
            if interrupt.cancelled() {
                return Err(ScriptError::Computer(ToolError::Cancelled));
            }
            let elapsed = start.elapsed();
            let left = budget.checked_sub(elapsed).ok_or(ToolError::TimedOut)?;
            let timeout_ms = left.as_millis().clamp(1, 30_000) as u32;
            let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
            let (action, space) = executor.resolve(action, self.session, id).await?;
            let request = ToolRequest {
                version: 1,
                id,
                session: self.session,
                timeout_ms,
                action,
            };
            let result = dispatcher.dispatch(&request, &space).await;
            let failed = result.error.is_some();
            results.push(result);
            if failed {
                break;
            }
        }
        Ok(ScriptOutcome {
            logs: plan.logs,
            results,
        })
    }
}

type WorkerState = (Runtime, Context, Arc<Mutex<Option<ExecutionContext>>>);

fn worker(rx: mpsc::Receiver<Work>, ready: mpsc::SyncSender<Result<(), ScriptError>>) {
    let result = (|| -> Result<WorkerState, ScriptError> {
        let runtime = Runtime::new().map_err(|e| ScriptError::JavaScript(e.to_string()))?;
        runtime.set_memory_limit(HEAP_LIMIT);
        runtime.set_max_stack_size(256 * 1024);
        let active = Arc::new(Mutex::new(None::<ExecutionContext>));
        let check = active.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            check
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_some_and(|ctx| ctx.check().is_err())
        })));
        let context =
            Context::full(&runtime).map_err(|e| ScriptError::JavaScript(e.to_string()))?;
        context
            .with(|js| js.eval::<(), _>(BOOTSTRAP))
            .map_err(|e| ScriptError::JavaScript(e.to_string()))?;
        Ok((runtime, context, active))
    })();
    let (runtime, context, active) = match result {
        Ok(v) => {
            let _ = ready.send(Ok(()));
            v
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    for item in rx {
        *active.lock().unwrap_or_else(|e| e.into_inner()) = Some(item.context.clone());
        let result = (|| -> Result<ScriptPlan, ScriptError> {
            item.context.check()?;
            let evaluated = context.with(|js| js.eval::<(), _>(item.source.as_str()));
            if let Err(e) = evaluated {
                // Do not execute any queued mutations after a JS error.
                let _ = context.with(|js| js.eval::<(), _>("__kaiDiscard()"));
                return Err(ScriptError::JavaScript(e.to_string()));
            }
            // Support resolved async helpers/microtasks without exposing an
            // unbounded event loop or timers. Async browser steps take effect
            // after this call, when the dispatcher has authorized them.
            let mut drained = false;
            for _ in 0..128 {
                item.context.check()?;
                if !runtime
                    .execute_pending_job()
                    .map_err(|e| ScriptError::JavaScript(format!("{e:?}")))?
                {
                    drained = true;
                    break;
                }
            }
            if !drained {
                return Err(ScriptError::Limit("unsettled JavaScript promises"));
            }
            let raw = context
                .with(|js| js.eval::<String, _>("__kaiDrain()"))
                .map_err(|e| ScriptError::JavaScript(e.to_string()))?;
            if raw.len() > MAX_OUTPUT {
                return Err(ScriptError::Limit("serialized script output"));
            }
            let plan: ScriptPlan = serde_json::from_str(&raw)
                .map_err(|_| ScriptError::Limit("invalid action data"))?;
            plan.validate()?;
            Ok(plan)
        })();
        // Discard also after validation errors, so no stale action can run.
        let _ = context.with(|js| js.eval::<(), _>("__kaiDiscard()"));
        *active.lock().unwrap_or_else(|e| e.into_inner()) = None;
        // Failed scripts may leave microtasks that could enqueue operations
        // on a later call. Close this realm rather than replay stale work.
        let failed = result.is_err();
        let _ = item.reply.send(result);
        if failed {
            break;
        }
    }
}

#[derive(Clone)]
struct Located {
    selector: String,
    session: String,
    url: String,
    document_start: f64,
    backend_id: u64,
    identity: String,
    x: f64,
    y: f64,
    width: u32,
    height: u32,
}

#[derive(Clone, PartialEq)]
struct PageState {
    session: String,
    url: String,
    document_start: f64,
    check_focus: bool,
    focus: Option<String>,
}

struct BrowserExecutor<'a> {
    browser: &'a BrowserLane,
    selector: Mutex<Option<Located>>,
    page_state: Mutex<Option<PageState>>,
}

impl BrowserExecutor<'_> {
    async fn page_state(&self, check_focus: bool) -> Result<PageState, ScriptError> {
        // Capture the current document and focused element before approval.
        // Typing into a replaced field or an unexpected navigated tab must
        // fail even if the viewport and pixel coordinates have not changed.
        let session = self.browser.current().session;
        let result = self
            .browser
            .page(
                "Runtime.evaluate",
                json!({
                    "expression": r#"(() => {
              const el = document.activeElement;
              return {url: location.href, document_start: performance.timeOrigin,
                focus: el ? JSON.stringify([el.tagName, el.id, el.getAttribute('name'),
                  el.getAttribute('type'), el.getAttribute('contenteditable'),
                  el.getAttribute('aria-label')]) : null};
            })()"#,
                    "returnByValue": true
                }),
            )
            .await
            .map_err(|_| ScriptError::Computer(ToolError::StaleObservation))?;
        let value = &result["result"]["value"];
        let (Some(url), Some(document_start)) =
            (value["url"].as_str(), value["document_start"].as_f64())
        else {
            return Err(ScriptError::Computer(ToolError::StaleObservation));
        };
        Ok(PageState {
            session,
            url: url.to_owned(),
            document_start,
            check_focus,
            focus: if check_focus {
                value["focus"].as_str().map(str::to_owned)
            } else {
                None
            },
        })
    }

    async fn locate(&self, selector: &str) -> Result<Located, ScriptError> {
        // Only this fixed, read-only DOM query runs in the webpage. Script
        // source is never evaluated in an authenticated origin.
        let encoded =
            serde_json::to_string(selector).map_err(|_| ScriptError::Limit("selector"))?;
        let expression = format!(
            r#"(() => {{
          const el = document.querySelector({encoded});
          if (!el) return null;
          const box = el.getBoundingClientRect();
          const x = box.left + box.width/2, y = box.top + box.height/2;
          if (box.width <= 0 || box.height <= 0 || !Number.isFinite(x) || !Number.isFinite(y) ||
              x < 0 || y < 0 || x >= innerWidth || y >= innerHeight ||
              el.disabled || !el.isConnected || el.getAttribute('aria-disabled') === 'true' ||
              getComputedStyle(el).pointerEvents === 'none' ||
              getComputedStyle(el).visibility === 'hidden' || getComputedStyle(el).display === 'none') return null;
          const top = document.elementFromPoint(x, y);
          if (!top || !(top === el || el.contains(top))) return null;
          const identity = JSON.stringify([el.tagName, el.id, el.getAttribute('href'),
            el.getAttribute('type'), el.getAttribute('aria-label'), el.textContent.slice(0,256)]);
          return {{x, y, width:innerWidth, height:innerHeight, url:location.href,
            document_start:performance.timeOrigin, identity}};
        }})()"#
        );
        let session = self.browser.current().session;
        let response = self
            .browser
            .page(
                "Runtime.evaluate",
                json!({
                    "expression": expression, "returnByValue": true
                }),
            )
            .await
            .map_err(|_| ScriptError::Selector(selector.to_owned()))?;
        // DOM backend IDs come from Chromium rather than webpage JavaScript.
        // They survive within an element's lifetime but change if the page
        // replaces a node with a visually identical copy during approval.
        let root = self
            .browser
            .page("DOM.getDocument", json!({"depth": 0}))
            .await
            .map_err(|_| ScriptError::Selector(selector.to_owned()))?;
        let root_id = root["root"]["nodeId"]
            .as_u64()
            .ok_or_else(|| ScriptError::Selector(selector.to_owned()))?;
        let found = self
            .browser
            .page(
                "DOM.querySelector",
                json!({"nodeId": root_id, "selector": selector}),
            )
            .await
            .map_err(|_| ScriptError::Selector(selector.to_owned()))?;
        let node_id = found["nodeId"]
            .as_u64()
            .filter(|id| *id != 0)
            .ok_or_else(|| ScriptError::Selector(selector.to_owned()))?;
        let described = self
            .browser
            .page("DOM.describeNode", json!({"nodeId": node_id}))
            .await
            .map_err(|_| ScriptError::Selector(selector.to_owned()))?;
        let backend_id = described["node"]["backendNodeId"]
            .as_u64()
            .ok_or_else(|| ScriptError::Selector(selector.to_owned()))?;
        let value = &response["result"]["value"];
        let (Some(x), Some(y), Some(w), Some(h), Some(url), Some(document_start), Some(identity)) = (
            value["x"].as_f64(),
            value["y"].as_f64(),
            value["width"].as_u64(),
            value["height"].as_u64(),
            value["url"].as_str(),
            value["document_start"].as_f64(),
            value["identity"].as_str(),
        ) else {
            return Err(ScriptError::Selector(selector.to_owned()));
        };
        let (width, height) = (w as u32, h as u32);
        if width == 0 || height == 0 || x >= width as f64 || y >= height as f64 {
            return Err(ScriptError::Selector(selector.to_owned()));
        }
        Ok(Located {
            selector: selector.to_owned(),
            session,
            url: url.to_owned(),
            document_start,
            backend_id,
            identity: identity.to_owned(),
            x,
            y,
            width,
            height,
        })
    }

    async fn resolve(
        &self,
        action: ScriptAction,
        session: u64,
        id: u64,
    ) -> Result<(ComputerAction, ObservationSpace), ScriptError> {
        let mut space = ObservationSpace {
            session,
            id,
            image_width: self.browser.size.0,
            image_height: self.browser.size.1,
            region: Rect::new(
                0.,
                0.,
                self.browser.size.0 as f32,
                self.browser.size.1 as f32,
            ),
        };
        let result = match action {
            ScriptAction::Click { selector } => {
                let found = self.locate(&selector).await?;
                space.image_width = found.width;
                space.image_height = found.height;
                space.region = Rect::new(0., 0., found.width as f32, found.height as f32);
                let target = Target::Point {
                    observation: id,
                    x: found.x,
                    y: found.y,
                };
                *self.selector.lock().unwrap_or_else(|e| e.into_inner()) = Some(found);
                *self.page_state.lock().unwrap_or_else(|e| e.into_inner()) = None;
                ComputerAction::Click {
                    target,
                    button: MouseButton::Left,
                    count: 1,
                    modifiers: vec![],
                }
            }
            ScriptAction::Type { text } => {
                *self.selector.lock().unwrap_or_else(|e| e.into_inner()) = None;
                let state = self.page_state(true).await?;
                *self.page_state.lock().unwrap_or_else(|e| e.into_inner()) = Some(state);
                ComputerAction::Type {
                    target: Target::Point {
                        observation: id,
                        x: 0.,
                        y: 0.,
                    },
                    text,
                }
            }
            ScriptAction::Press { keys } => {
                *self.selector.lock().unwrap_or_else(|e| e.into_inner()) = None;
                let state = self.page_state(true).await?;
                *self.page_state.lock().unwrap_or_else(|e| e.into_inner()) = Some(state);
                ComputerAction::Keypress { keys }
            }
            ScriptAction::Open { url } => {
                *self.selector.lock().unwrap_or_else(|e| e.into_inner()) = None;
                let state = self.page_state(false).await?;
                *self.page_state.lock().unwrap_or_else(|e| e.into_inner()) = Some(state);
                ComputerAction::OpenUrl { url }
            }
            ScriptAction::Wait { duration_ms } => {
                *self.selector.lock().unwrap_or_else(|e| e.into_inner()) = None;
                *self.page_state.lock().unwrap_or_else(|e| e.into_inner()) = None;
                ComputerAction::Wait { duration_ms }
            }
        };
        Ok((result, space))
    }
}

#[async_trait::async_trait]
impl ComputerExecutor for BrowserExecutor<'_> {
    fn capabilities(&self) -> Capabilities {
        let mut caps = self.browser.computer_capabilities();
        // No privilege escalation through browser script plans.
        caps.actions.retain(|a| {
            matches!(
                a,
                ActionKind::Click
                    | ActionKind::Type
                    | ActionKind::Keypress
                    | ActionKind::OpenUrl
                    | ActionKind::Wait
            )
        });
        caps
    }
    async fn is_current(
        &self,
        _: &ComputerAction,
        _: &ObservationSpace,
    ) -> Result<bool, ToolError> {
        let page_snapshot = self
            .page_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(snapshot) = page_snapshot {
            if self.browser.current().session != snapshot.session {
                return Ok(false);
            }
            return Ok(self
                .page_state(snapshot.check_focus)
                .await
                .is_ok_and(|now| now == snapshot));
        }
        let snapshot = self
            .selector
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let Some(snapshot) = snapshot else {
            return Ok(true);
        };
        if self.browser.current().session != snapshot.session {
            return Ok(false);
        }
        let Ok(now) = self.locate(&snapshot.selector).await else {
            return Ok(false);
        };
        Ok(now.session == snapshot.session
            && now.url == snapshot.url
            && now.document_start == snapshot.document_start
            && now.backend_id == snapshot.backend_id
            && now.identity == snapshot.identity
            && now.width == snapshot.width
            && now.height == snapshot.height
            && (now.x - snapshot.x).abs() < 2.0
            && (now.y - snapshot.y).abs() < 2.0)
    }
    async fn execute(
        &self,
        action: &ComputerAction,
        space: &ObservationSpace,
        context: &ExecutionContext,
    ) -> Result<(), ToolError> {
        self.browser
            .computer_act(action, space, &Grounding::Marks(vec![]), context)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_preserves_state_rejects_host_apis_and_limits_infinite_loops() {
        let script = BrowserScript::new().unwrap();
        let call = |code: &str, ms| {
            let (tx, rx) = mpsc::sync_channel(1);
            script
                .tx
                .send(Work {
                    source: code.into(),
                    context: ExecutionContext::new(Duration::from_millis(ms)),
                    reply: tx,
                })
                .unwrap();
            rx.recv().unwrap()
        };
        let a = call("let suffix='later'; kai.click('#start')", 200).unwrap();
        assert_eq!(a.actions.len(), 1);
        let b = call(
            "kai.click('#'+suffix); kai.log(typeof process, typeof require, typeof fetch)",
            200,
        )
        .unwrap();
        assert!(matches!(&b.actions[0],ScriptAction::Click {selector} if selector == "#later"));
        assert_eq!(b.logs, vec!["undefined undefined undefined"]);
        assert!(call("while(true){}", 10).is_err());
        // A broken script realm cannot carry stale async jobs into a retry.
        let (tx, rx) = mpsc::sync_channel(1);
        if script
            .tx
            .send(Work {
                source: "kai.log('still working')".into(),
                context: ExecutionContext::new(Duration::from_millis(200)),
                reply: tx,
            })
            .is_ok()
        {
            assert!(
                rx.recv().is_err(),
                "closed realm cannot accept another plan"
            );
        }
        let script = BrowserScript::new().unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        script
            .tx
            .send(Work {
                source: "for(let i=0;i<40;i++) kai.click('#x')".into(),
                context: ExecutionContext::new(Duration::from_millis(200)),
                reply: tx,
            })
            .unwrap();
        assert!(rx.recv().unwrap().is_err());
    }
}
