//! Fakes shared by the fast-loop tests: a counting model client, a rich
//! scripted screen (Set-of-Marks or grid), recording hands, a recording sink.
#![allow(dead_code)]

use async_trait::async_trait;
use kai_core::client::{ClientError, ModelReply, ModelRequest};
use kai_core::engine::{
    Actuator, ApprovalChoice, Approver, Engine, EngineEvent, EventSink, Interrupt, NoSteering,
    Observation, Perceptor, Steering,
};
use kai_core::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// What one model request looked like.
#[derive(Debug, Clone)]
pub struct Seen {
    pub had_image: bool,
    pub had_crop: bool,
    pub text: String,
}

/// Wraps [`MockClient`] and records every call.
pub struct Counting {
    pub inner: MockClient,
    pub calls: Mutex<Vec<Seen>>,
    pub backend: Backend,
}

impl Counting {
    pub fn new(replies: &[&str]) -> Self {
        Self {
            inner: MockClient::new(replies.iter().map(|s| s.to_string())),
            calls: Mutex::new(Vec::new()),
            backend: Backend::Mock,
        }
    }
    pub fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
    pub fn seen(&self) -> Vec<Seen> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl ModelClient for Counting {
    fn backend(&self) -> Backend {
        self.backend
    }
    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        self.calls.lock().unwrap().push(Seen {
            had_image: !req.images.screenshot_jpeg.is_empty(),
            had_crop: req.images.crop_jpeg.is_some(),
            text: req.user_text.to_string(),
        });
        self.inner.decide(req).await
    }
}

/// A screen with ten named elements (or a grid). `frame` advances on every
/// action; from frame `rename_at` on, element #3 is renamed.
pub struct Screen {
    pub frame: AtomicUsize,
    pub grid: bool,
    pub rename_at: usize,
    pub full: AtomicUsize,
    pub tree: AtomicUsize,
    pub zooms: AtomicUsize,
}

impl Screen {
    pub fn marks() -> Self {
        Self {
            frame: AtomicUsize::new(0),
            grid: false,
            rename_at: usize::MAX,
            full: AtomicUsize::new(0),
            tree: AtomicUsize::new(0),
            zooms: AtomicUsize::new(0),
        }
    }
    pub fn grid() -> Self {
        Self {
            grid: true,
            ..Self::marks()
        }
    }
    fn grounding(&self) -> Grounding {
        if self.grid {
            return Grounding::Grid {
                cols: 4,
                rows: 4,
                screen: Rect::new(0.0, 0.0, 400.0, 400.0),
            };
        }
        let f = self.frame.load(Ordering::SeqCst);
        let names = [
            "File", "Edit", "Save", "Search", "Delete", "Back", "Forward", "Reload", "Home", "Help",
        ];
        Grounding::Marks(
            names
                .iter()
                .enumerate()
                .map(|(i, n)| Element {
                    mark: i as u32 + 1,
                    role: if *n == "Search" {
                        Role::TextField
                    } else {
                        Role::Button
                    },
                    name: if i == 2 && f >= self.rename_at {
                        "Saved!".into()
                    } else if i == 9 {
                        format!("Help (frame {f})")
                    } else {
                        n.to_string()
                    },
                    bounds: Rect::new(10.0 * i as f32, 0.0, 10.0, 10.0),
                    enabled: true,
                    secure: false,
                    value: None,
                })
                .collect(),
        )
    }
    fn obs(&self, image: bool) -> Observation {
        Observation {
            images: Images {
                screenshot_jpeg: if image { vec![0xff, 0xd8] } else { Vec::new() },
                crop_jpeg: None,
            },
            grounding: self.grounding(),
            screen_hash: self.frame.load(Ordering::SeqCst) as u64,
            timing: Default::default(),
            screen: None,
        }
    }
}

#[async_trait]
impl Perceptor for Screen {
    async fn observe(&self) -> anyhow::Result<Observation> {
        self.full.fetch_add(1, Ordering::SeqCst);
        Ok(self.obs(true))
    }
    async fn observe_tree_only(&self) -> anyhow::Result<Observation> {
        self.tree.fetch_add(1, Ordering::SeqCst);
        Ok(self.obs(false))
    }
    async fn zoom(&self, _rect: Rect) -> Option<Vec<u8>> {
        self.zooms.fetch_add(1, Ordering::SeqCst);
        Some(vec![0xff, 0xd8, 0x01])
    }
}

/// Records actions; advances the screen unless `frozen`; can take the a11y
/// path; can raise the interrupt after `stop_after` actions.
pub struct Hands<'a> {
    pub screen: &'a Screen,
    pub log: Mutex<Vec<String>>,
    pub a11y: bool,
    pub frozen: bool,
    pub stop: Option<(&'a Flag, usize)>,
}

impl<'a> Hands<'a> {
    pub fn new(screen: &'a Screen) -> Self {
        Self {
            screen,
            log: Mutex::new(Vec::new()),
            a11y: false,
            frozen: false,
            stop: None,
        }
    }
    fn did(&self, what: String) {
        if !self.frozen {
            self.screen.frame.fetch_add(1, Ordering::SeqCst);
        }
        let mut log = self.log.lock().unwrap();
        log.push(what);
        if let Some((flag, n)) = self.stop
            && log.len() >= n
        {
            flag.0.store(true, Ordering::SeqCst);
        }
    }
    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

#[async_trait]
impl Actuator for Hands<'_> {
    async fn act(&self, d: &Decision, _g: &Grounding) -> anyhow::Result<()> {
        self.did(format!("{:?} {:?} {:?}", d.action, d.target, d.keys));
        Ok(())
    }
    async fn open_app(&self, a: &str) -> anyhow::Result<()> {
        self.did(format!("open_app {a}"));
        Ok(())
    }
    async fn open_url(&self, u: &str) -> anyhow::Result<()> {
        self.did(format!("open_url {u}"));
        Ok(())
    }
    async fn shell(&self, c: &str) -> anyhow::Result<String> {
        self.did(format!("shell {c}"));
        Ok(format!("ran {c}"))
    }
    async fn a11y_act(&self, d: &Decision, _g: &Grounding) -> anyhow::Result<bool> {
        if self.a11y {
            self.did(format!("a11y {:?} {:?}", d.action, d.target));
        }
        Ok(self.a11y)
    }
}

pub struct Approve(pub ApprovalChoice, pub AtomicUsize);
#[async_trait]
impl Approver for Approve {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0
    }
}
pub fn approve_all() -> Approve {
    Approve(ApprovalChoice::Approve, AtomicUsize::new(0))
}

pub struct Flag(pub AtomicBool);
impl Interrupt for Flag {
    fn stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
pub fn flag() -> Flag {
    Flag(AtomicBool::new(false))
}

#[derive(Default)]
pub struct Events(pub Mutex<Vec<String>>);
impl EventSink for Events {
    fn emit(&self, ev: &EngineEvent) {
        let s = match ev {
            EngineEvent::Steered { message, .. } => format!("steered {message}"),
            EngineEvent::Output {
                command, output, ..
            } => format!("output {command} => {output}"),
            EngineEvent::Finished { outcome } => format!("finished {outcome:?}"),
            _ => return,
        };
        self.0.lock().unwrap().push(s);
    }
}

/// Steering that hands out one message on the first drain.
pub struct Once(pub Mutex<Vec<String>>);
impl Steering for Once {
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

pub fn engine<'a>(
    client: &'a dyn ModelClient,
    screen: &'a Screen,
    hands: &'a dyn Actuator,
    approver: &'a dyn Approver,
    stop: &'a dyn Interrupt,
) -> Engine<'a> {
    Engine {
        client,
        careful: None,
        perceptor: screen,
        actuator: hands,
        approver,
        interrupt: stop,
        sink: &kai_core::engine::NullSink,
        steering: &NoSteering,
        verifier: None,
        human: None,
        skills: &[],
        config: Config::default(),
    }
}
