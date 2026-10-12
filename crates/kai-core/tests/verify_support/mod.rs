//! Fakes for the verification tests: a probe port with scripted verdicts
//! (or one that hangs), a screen that counts accessibility events, a
//! scripted person, a page with a one-time-code field, and a sink that
//! keeps every UI event as text.
#![allow(dead_code)]

use crate::common::{Hands, Screen};
use async_trait::async_trait;
use kai_core::engine::{Actuator, EngineEvent, EventSink, Observation, Perceptor};
use kai_core::human::{HumanCheck, HumanHelper, HumanReply};
use kai_core::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// A probe port: answers `ok` (or hangs), and records every probe it got.
pub struct FakeProbe {
    pub ok: bool,
    pub hang: bool,
    pub got: Mutex<Vec<String>>,
}

impl FakeProbe {
    pub fn answering(ok: bool) -> Self {
        Self {
            ok,
            hang: false,
            got: Mutex::new(Vec::new()),
        }
    }
    pub fn hanging() -> Self {
        Self {
            hang: true,
            ..Self::answering(true)
        }
    }
    pub fn calls(&self) -> Vec<String> {
        self.got.lock().unwrap().clone()
    }
}

#[async_trait]
impl Probe for FakeProbe {
    async fn check(&self, p: &ProbeSpec) -> anyhow::Result<ProbeVerdict> {
        self.got.lock().unwrap().push(p.label());
        if self.hang {
            std::future::pending::<()>().await;
        }
        Ok(if self.ok {
            ProbeVerdict::pass("exit 0")
        } else {
            ProbeVerdict::fail("exit 1")
        })
    }
}

/// The common screen, plus an accessibility-event counter the hands bump
/// (`None` when events are off).
pub struct Eventful<'a> {
    pub screen: &'a Screen,
    pub events: Option<&'a AtomicU64>,
}

#[async_trait]
impl Perceptor for Eventful<'_> {
    async fn observe(&self) -> anyhow::Result<Observation> {
        self.screen.observe().await
    }
    async fn observe_tree_only(&self) -> anyhow::Result<Observation> {
        self.screen.observe_tree_only().await
    }
    fn ui_events(&self) -> Option<u64> {
        self.events.map(|e| e.load(Ordering::SeqCst))
    }
}

/// Hands whose every action also fires one accessibility event.
pub struct Firing<'a> {
    pub hands: &'a Hands<'a>,
    pub events: &'a AtomicU64,
}

#[async_trait]
impl Actuator for Firing<'_> {
    async fn act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        self.events.fetch_add(1, Ordering::SeqCst);
        self.hands.act(d, g).await
    }
    async fn open_app(&self, a: &str) -> anyhow::Result<()> {
        self.hands.open_app(a).await
    }
    async fn open_url(&self, u: &str) -> anyhow::Result<()> {
        self.hands.open_url(u).await
    }
    async fn shell(&self, c: &str) -> anyhow::Result<String> {
        self.hands.shell(c).await
    }
}

/// A person who answers in order, then stops the run.
pub struct Person {
    pub script: Mutex<Vec<HumanReply>>,
    pub asked: Mutex<Vec<HumanCheck>>,
}

impl Person {
    pub fn new(script: Vec<HumanReply>) -> Self {
        Self {
            script: Mutex::new(script),
            asked: Mutex::new(Vec::new()),
        }
    }
    pub fn asked(&self) -> Vec<String> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.prompt.clone())
            .collect()
    }
}

#[async_trait]
impl HumanHelper for Person {
    async fn ask(&self, check: &HumanCheck) -> HumanReply {
        self.asked.lock().unwrap().push(check.clone());
        let mut s = self.script.lock().unwrap();
        if s.is_empty() {
            HumanReply::Stop
        } else {
            s.remove(0)
        }
    }
}

/// Every UI event, as text.
#[derive(Default)]
pub struct Sink(pub Mutex<Vec<String>>);
impl EventSink for Sink {
    fn emit(&self, ev: &EngineEvent) {
        self.0.lock().unwrap().push(format!("{ev:?}"));
    }
}
impl Sink {
    pub fn all(&self) -> String {
        self.0.lock().unwrap().join("\n")
    }
}

/// A sign-in page with a verification-code field. Typing lands in the
/// field's value (which the engine must redact once it is a code).
pub struct CodePage {
    pub typed: Mutex<Option<String>>,
    pub looks: AtomicUsize,
}

impl CodePage {
    pub fn new() -> Self {
        Self {
            typed: Mutex::new(None),
            looks: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Perceptor for CodePage {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let n = self.looks.fetch_add(1, Ordering::SeqCst);
        let el = |mark: u32, role: Role, name: &str, value: Option<String>| Element {
            mark,
            role,
            name: name.into(),
            bounds: Rect::new(10.0 * mark as f32, 0.0, 10.0, 10.0),
            enabled: true,
            secure: false,
            value,
        };
        Ok(Observation {
            images: Images {
                screenshot_jpeg: vec![0xff, 0xd8],
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(vec![
                el(1, Role::Other, "We sent a code to your phone", None),
                el(
                    2,
                    Role::TextField,
                    "Verification code",
                    self.typed.lock().unwrap().clone(),
                ),
                el(3, Role::Button, "Verify", None),
            ]),
            screen_hash: n as u64,
            timing: Default::default(),
            screen: None,
            window_title: Some("Sign in".into()),
        })
    }
}

/// Hands that type into the code page.
pub struct CodeHands<'a>(pub &'a CodePage);

#[async_trait]
impl Actuator for CodeHands<'_> {
    async fn act(&self, d: &Decision, _g: &Grounding) -> anyhow::Result<()> {
        if d.action == Action::Type {
            *self.0.typed.lock().unwrap() = d.text.clone();
        }
        Ok(())
    }
    async fn open_app(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn open_url(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn shell(&self, _: &str) -> anyhow::Result<String> {
        Ok("[exit 0]".into())
    }
}
