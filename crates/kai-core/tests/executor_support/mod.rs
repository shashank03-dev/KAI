//! Fakes for the executor tests: a web-form screen whose fields hold what was
//! typed, hands that type into it, and a scripted decision model.
#![allow(dead_code)]

use async_trait::async_trait;
use kai_core::client::{Answer, ClientError, Question};
use kai_core::engine::{Actuator, Observation, Perceptor};
use kai_core::executor::Decider;
use kai_core::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const NAMES: &[(&str, Role)] = &[
    ("Home", Role::Link),
    ("Search", Role::TextField),
    ("Name", Role::TextField),
    ("Email", Role::TextField),
    ("Message", Role::TextField),
    ("Subscribe", Role::Checkbox),
    ("Send message", Role::Button),
    ("Cancel", Role::Button),
    ("Password", Role::PasswordField),
    ("Gemma 4 model card", Role::Link),
];

/// Marks are 1-based in `NAMES` order: Search #2, Name #3, Email #4,
/// Message #5, Send #7, Password #9, result link #10.
#[derive(Default)]
pub struct Form {
    pub values: Mutex<BTreeMap<u32, String>>,
    pub frame: AtomicUsize,
    pub full: AtomicUsize,
    pub tree: AtomicUsize,
}

impl Form {
    fn obs(&self, image: bool) -> Observation {
        let values = self.values.lock().unwrap();
        let els = NAMES
            .iter()
            .enumerate()
            .map(|(i, (n, role))| {
                let mark = i as u32 + 1;
                Element {
                    mark,
                    role: *role,
                    name: n.to_string(),
                    bounds: Rect::new(10.0, 40.0 * i as f32, 200.0, 30.0),
                    enabled: true,
                    secure: *role == Role::PasswordField,
                    value: values.get(&mark).cloned(),
                }
            })
            .collect();
        Observation {
            images: Images {
                screenshot_jpeg: if image { vec![0xff, 0xd8] } else { Vec::new() },
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(els),
            screen_hash: self.frame.load(Ordering::SeqCst) as u64,
            timing: Default::default(),
            screen: None,
        }
    }
}

#[async_trait]
impl Perceptor for Form {
    async fn observe(&self) -> anyhow::Result<Observation> {
        self.full.fetch_add(1, Ordering::SeqCst);
        Ok(self.obs(true))
    }
    async fn observe_tree_only(&self) -> anyhow::Result<Observation> {
        self.tree.fetch_add(1, Ordering::SeqCst);
        Ok(self.obs(false))
    }
}

pub struct FormHands<'a> {
    pub form: &'a Form,
    pub log: Mutex<Vec<String>>,
}

impl<'a> FormHands<'a> {
    pub fn new(form: &'a Form) -> Self {
        Self {
            form,
            log: Mutex::new(Vec::new()),
        }
    }
    fn did(&self, s: String) {
        self.form.frame.fetch_add(1, Ordering::SeqCst);
        self.log.lock().unwrap().push(s);
    }
    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

#[async_trait]
impl Actuator for FormHands<'_> {
    async fn act(&self, d: &Decision, _g: &Grounding) -> anyhow::Result<()> {
        if let (Action::Type, Some(t), Some(text)) = (d.action, d.target, &d.text) {
            self.form.values.lock().unwrap().insert(t, text.clone());
        }
        let what = match d.action {
            Action::Type => format!(
                "type #{} {}",
                d.target.unwrap_or(0),
                d.text.clone().unwrap_or_default()
            ),
            Action::Key => format!("key {}", d.keys.join("+")),
            a => format!("{a:?} #{}", d.target.unwrap_or(0)).to_lowercase(),
        };
        self.did(what);
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
        Ok("[exit 0]".into())
    }
}

/// A scripted decision model: picks the first offered target option with
/// `target_p` (runner-up gets `second_p`), and answers `ready`,
/// `already_done` and `expect_met` with fixed numbers.
pub struct FakeJevos {
    pub target_p: f64,
    pub second_p: f64,
    pub ready: f64,
    pub done: f64,
    pub met: f64,
    pub healthy: bool,
    pub asks: AtomicUsize,
    pub seen: Mutex<Vec<Vec<String>>>,
}

impl FakeJevos {
    pub fn confident() -> Self {
        Self {
            target_p: 0.9,
            second_p: 0.05,
            ready: 0.95,
            done: 0.05,
            met: 0.92,
            healthy: true,
            asks: AtomicUsize::new(0),
            seen: Mutex::new(Vec::new()),
        }
    }
    pub fn count(&self) -> usize {
        self.asks.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Decider for FakeJevos {
    async fn ask(
        &self,
        _state: &serde_json::Value,
        questions: &[(String, Question)],
    ) -> Result<BTreeMap<String, Answer>, ClientError> {
        self.asks.fetch_add(1, Ordering::SeqCst);
        self.seen
            .lock()
            .unwrap()
            .push(questions.iter().map(|q| q.0.clone()).collect());
        let mut out = BTreeMap::new();
        for (id, q) in questions {
            let a = match q {
                Question::Criteria { criteria, .. } => {
                    let first = criteria[0].0.clone();
                    let mut probabilities = BTreeMap::new();
                    probabilities.insert(first.clone(), self.target_p);
                    if let Some(second) = criteria.get(1) {
                        probabilities.insert(second.0.clone(), self.second_p);
                    }
                    Answer::Choice {
                        option: first,
                        probabilities,
                        confidence: Some(self.target_p),
                    }
                }
                _ => Answer::Noul(match id.as_str() {
                    "ready" => self.ready,
                    "already_done" => self.done,
                    _ => self.met,
                }),
            };
            out.insert(id.clone(), a);
        }
        Ok(out)
    }

    async fn health(&self) -> Result<Duration, ClientError> {
        if self.healthy {
            Ok(Duration::from_millis(1))
        } else {
            Err(ClientError::Transport("connection refused".into()))
        }
    }
}
