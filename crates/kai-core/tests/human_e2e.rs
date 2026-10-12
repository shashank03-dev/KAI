//! Human checks end to end through the real engine: a CAPTCHA, a one-time
//! code, a sign-in question. Fake screen, fake hands, scripted model and a
//! scripted person. See `docs/HUMAN_CHECKS.md`.

mod common;

use async_trait::async_trait;
use common::{Counting, Flag, approve_all, flag};
use kai_core::engine::{
    Actuator, Engine, EngineEvent, EventSink, NoSteering, Observation, Perceptor,
};
use kai_core::human::{CheckKind, HumanCheck, HumanHelper, HumanReply, OneTimeCode, REDACTED_CODE};
use kai_core::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const CAPTCHA: usize = 0;
const WELCOME: usize = 1;
const CODE: usize = 2;
const PLAIN: usize = 3;

const THE_CODE: &str = "482913";

/// A page that changes when the person (or Kai) does something.
struct Page {
    stage: AtomicUsize,
    typed: Mutex<Option<String>>,
}

impl Page {
    fn at(stage: usize) -> Self {
        Self {
            stage: AtomicUsize::new(stage),
            typed: Mutex::new(None),
        }
    }
    fn go(&self, stage: usize) {
        self.stage.store(stage, Ordering::SeqCst);
    }
    fn grounding(&self) -> Grounding {
        let el = |mark: u32, role: Role, name: &str| Element {
            mark,
            role,
            name: name.into(),
            bounds: Rect::new(10.0 * mark as f32, 0.0, 10.0, 10.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        };
        let els = match self.stage.load(Ordering::SeqCst) {
            CAPTCHA => vec![
                el(1, Role::Checkbox, "I'm not a robot"),
                el(2, Role::Button, "Submit"),
            ],
            WELCOME => vec![el(1, Role::Button, "Inbox"), el(2, Role::Link, "Settings")],
            CODE => {
                let mut field = el(2, Role::TextField, "Verification code");
                field.value = self.typed.lock().unwrap().clone();
                vec![
                    el(1, Role::Other, "We sent a code to your phone"),
                    field,
                    el(3, Role::Button, "Verify"),
                ]
            }
            _ => vec![el(1, Role::TextField, "Search"), el(2, Role::Button, "Go")],
        };
        Grounding::Marks(els)
    }
}

#[async_trait]
impl Perceptor for Page {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let typed = self.typed.lock().unwrap().as_ref().map_or(0, String::len);
        Ok(Observation {
            images: Images {
                screenshot_jpeg: vec![0xff, 0xd8],
                crop_jpeg: None,
            },
            grounding: self.grounding(),
            screen_hash: (self.stage.load(Ordering::SeqCst) * 1000 + typed) as u64,
            timing: Default::default(),
            screen: None,
            window_title: None,
        })
    }
}

/// Records every action with its text; typing lands in the page. `then_go`
/// moves the page on after the first action (sign-in, then a code page).
struct Hands<'a> {
    page: &'a Page,
    log: Mutex<Vec<String>>,
    then_go: Mutex<Option<usize>>,
}

impl<'a> Hands<'a> {
    fn new(page: &'a Page) -> Self {
        Self {
            page,
            log: Mutex::new(Vec::new()),
            then_go: Mutex::new(None),
        }
    }
    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

#[async_trait]
impl Actuator for Hands<'_> {
    async fn act(&self, d: &Decision, _g: &Grounding) -> anyhow::Result<()> {
        let text = d.text.clone().unwrap_or_default();
        if d.action == Action::Type {
            *self.page.typed.lock().unwrap() = Some(text.clone());
        }
        self.log
            .lock()
            .unwrap()
            .push(format!("{:?} {:?} {text}", d.action, d.target));
        if let Some(stage) = self.then_go.lock().unwrap().take() {
            self.page.go(stage);
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
        Ok(String::new())
    }
}

/// A scripted person: answers in order, and can change the page first
/// (as if they solved the CAPTCHA themselves).
struct Person<'a> {
    page: &'a Page,
    script: Mutex<Vec<(Option<usize>, Reply)>>,
    asked: Mutex<Vec<HumanCheck>>,
    stop: Option<&'a Flag>,
    delay: Duration,
}

enum Reply {
    Now(HumanReply),
    Never,
}

impl<'a> Person<'a> {
    fn new(page: &'a Page, script: Vec<(Option<usize>, Reply)>) -> Self {
        Self {
            page,
            script: Mutex::new(script),
            asked: Mutex::new(Vec::new()),
            stop: None,
            delay: Duration::ZERO,
        }
    }
    fn kinds(&self) -> Vec<CheckKind> {
        self.asked.lock().unwrap().iter().map(|c| c.kind).collect()
    }
}

#[async_trait]
impl HumanHelper for Person<'_> {
    async fn ask(&self, check: &HumanCheck) -> HumanReply {
        self.asked.lock().unwrap().push(check.clone());
        let next = {
            let mut s = self.script.lock().unwrap();
            (!s.is_empty()).then(|| s.remove(0))
        };
        if let Some(f) = self.stop {
            f.0.store(true, Ordering::SeqCst);
        }
        tokio::time::sleep(self.delay).await;
        match next {
            Some((go, Reply::Now(r))) => {
                if let Some(stage) = go {
                    self.page.go(stage);
                }
                r
            }
            Some((_, Reply::Never)) | None => std::future::pending().await,
        }
    }
}

#[derive(Default)]
struct Seen(Mutex<Vec<String>>);
impl EventSink for Seen {
    fn emit(&self, ev: &EngineEvent) {
        // Everything the UI would see, as text.
        self.0.lock().unwrap().push(format!("{ev:?}"));
    }
}

fn engine<'a>(
    client: &'a Counting,
    page: &'a Page,
    hands: &'a Hands<'a>,
    stop: &'a Flag,
    human: Option<&'a dyn HumanHelper>,
    sink: &'a Seen,
) -> Engine<'a> {
    Engine {
        client,
        careful: None,
        perceptor: page,
        actuator: hands,
        approver: Box::leak(Box::new(approve_all())),
        interrupt: stop,
        sink,
        steering: &NoSteering,
        verifier: None,
        human,
        skills: &[],
        config: Config {
            tree_only: false,
            ..Config::default()
        },
    }
}

const DONE: &str = r#"{"thought":"finished","action":"done","risk":"none"}"#;

#[tokio::test]
async fn captcha_pauses_before_the_model_and_resumes_after_the_person() {
    let page = Page::at(CAPTCHA);
    let hands = Hands::new(&page);
    let model = Counting::new(&[DONE]);
    let person = Person::new(
        &page,
        vec![(Some(WELCOME), Reply::Now(HumanReply::Resolved))],
    );
    let (stop, sink) = (flag(), Seen::default());
    let trace = engine(&model, &page, &hands, &stop, Some(&person), &sink)
        .run("log in to my mail")
        .await;

    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(person.kinds(), vec![CheckKind::Captcha]);
    assert!(
        hands.log().is_empty(),
        "Kai touched the CAPTCHA: {:?}",
        hands.log()
    );
    // The model was only asked after the person was done, and was told so.
    assert_eq!(model.count(), 1);
    assert!(model.seen()[0].text.contains("completed the CAPTCHA"));
    assert_eq!(trace.human.len(), 1);
    assert_eq!(trace.human[0].result, "resolved");
    assert_eq!(trace.human[0].kind, CheckKind::Captcha);
    assert_eq!(trace.steps[0].path.as_deref(), Some("human"));
    assert_eq!(trace.steps[0].backend, kai_core::engine::HUMAN);
    let events = sink.0.lock().unwrap().join("\n");
    assert!(events.contains("NeedsHuman"));
    assert!(events.contains("HumanDone"));
}

#[tokio::test]
async fn headless_runs_end_as_asked_user_and_say_why() {
    let page = Page::at(CAPTCHA);
    let hands = Hands::new(&page);
    let model = Counting::new(&[DONE]);
    let (stop, sink) = (flag(), Seen::default());
    let trace = engine(&model, &page, &hands, &stop, None, &sink)
        .run("log in to my mail")
        .await;

    assert_eq!(trace.outcome, Some(Outcome::AskedUser));
    assert_eq!(model.count(), 0, "no model call is spent on a CAPTCHA");
    assert!(hands.log().is_empty());
    assert_eq!(trace.human[0].result, "no_one_to_ask");
    let f = kai_core::failure::classify(&trace, None).unwrap();
    assert_eq!(f.category, Category::NeedsHuman);
    assert!(f.detail.contains("captcha"), "{}", f.detail);
}

#[tokio::test]
async fn the_model_never_clicks_a_handled_captcha_and_asks_when_stuck() {
    let page = Page::at(CAPTCHA);
    let hands = Hands::new(&page);
    let click = r#"{"thought":"tick it","action":"click","target":1,"risk":"low"}"#;
    let model = Counting::new(&[click, click, click, DONE]);
    // The person ticks the box but the page stays (the box is still there).
    let person = Person::new(
        &page,
        vec![
            (None, Reply::Now(HumanReply::Resolved)),
            (
                Some(WELCOME),
                Reply::Now(HumanReply::Answer("just submit".into())),
            ),
        ],
    );
    let (stop, sink) = (flag(), Seen::default());
    let trace = engine(&model, &page, &hands, &stop, Some(&person), &sink)
        .run("log in to my mail")
        .await;

    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert!(hands.log().is_empty(), "clicked: {:?}", hands.log());
    assert_eq!(
        person.kinds(),
        vec![CheckKind::Captcha, CheckKind::Question]
    );
    let seen = model.seen();
    assert!(seen[1].text.contains("already handled"));
    assert!(
        seen[3]
            .text
            .contains("The user answered your question: just submit")
    );
}

#[tokio::test]
async fn a_code_is_typed_for_the_person_and_never_reaches_the_model_or_trace() {
    let page = Page::at(CODE);
    let hands = Hands::new(&page);
    let verify = r#"{"thought":"verify","action":"click","target":3,"risk":"low"}"#;
    let model = Counting::new(&[verify, DONE]);
    let code = OneTimeCode::parse("482 913").unwrap();
    let person = Person::new(&page, vec![(None, Reply::Now(HumanReply::Code(code)))]);
    let (stop, sink) = (flag(), Seen::default());
    let trace = engine(&model, &page, &hands, &stop, Some(&person), &sink)
        .run("sign in to the bank")
        .await;

    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(person.kinds(), vec![CheckKind::OneTimeCode]);
    // Typed once, into the code field (#2), then the model clicked Verify.
    assert_eq!(
        hands.log(),
        vec![
            format!("Type Some(2) {THE_CODE}"),
            "Click Some(3) ".to_string()
        ]
    );
    assert_eq!(trace.human[0].result, "code_typed");
    // Never in a model request, the trace, or a UI event.
    for s in model.seen() {
        assert!(!s.text.contains(THE_CODE), "model saw the code: {}", s.text);
    }
    let json = serde_json::to_string(&trace).unwrap();
    assert!(!json.contains(THE_CODE), "trace kept the code");
    assert!(json.contains(REDACTED_CODE));
    let events = sink.0.lock().unwrap().join("\n");
    assert!(!events.contains(THE_CODE), "an event carried the code");
    // Typed once and handled: not asked again although the field is still
    // on screen.
    assert_eq!(trace.human.len(), 1);
}

#[tokio::test]
async fn a_code_field_that_vanished_types_nothing() {
    let page = Page::at(CODE);
    let hands = Hands::new(&page);
    let model = Counting::new(&[DONE]);
    let code = OneTimeCode::parse(THE_CODE).unwrap();
    // The page moved on while the person was reading the SMS.
    let person = Person::new(
        &page,
        vec![(Some(PLAIN), Reply::Now(HumanReply::Code(code)))],
    );
    let (stop, sink) = (flag(), Seen::default());
    let trace = engine(&model, &page, &hands, &stop, Some(&person), &sink)
        .run("sign in to the bank")
        .await;

    assert!(hands.log().is_empty(), "typed: {:?}", hands.log());
    assert_eq!(trace.human[0].result, "code_field_missing");
    assert_eq!(trace.outcome, Some(Outcome::Success));
}

#[tokio::test]
async fn the_model_cannot_guess_a_code() {
    let page = Page::at(PLAIN);
    let hands = Hands::new(&page);
    // The code page appears only after the model's first action.
    let search = r#"{"thought":"sign in","action":"click","target":2,"risk":"low"}"#;
    let guess = r#"{"thought":"guess","action":"type","target":2,"text":"000000","risk":"low"}"#;
    let model = Counting::new(&[search, guess, DONE]);
    *hands.then_go.lock().unwrap() = Some(CODE);
    // The person enters the code on the page themselves; the field stays.
    let person = Person::new(&page, vec![(None, Reply::Now(HumanReply::Resolved))]);
    let (stop, sink) = (flag(), Seen::default());
    let trace = engine(&model, &page, &hands, &stop, Some(&person), &sink)
        .run("sign in")
        .await;
    assert_eq!(hands.log(), vec!["Click Some(2) ".to_string()]);
    assert!(model.seen()[2].text.contains("already handled"));
    assert!(
        !hands.log().iter().any(|l| l.contains("000000")),
        "Kai typed a guessed code: {:?}",
        hands.log()
    );
    assert_eq!(person.kinds(), vec![CheckKind::OneTimeCode]);
    assert_eq!(trace.outcome, Some(Outcome::Success));
}

#[tokio::test]
async fn ask_user_is_a_real_question_now() {
    let page = Page::at(PLAIN);
    let hands = Hands::new(&page);
    let ask = r#"{"thought":"which folder?","action":"ask_user","text":"Which folder should I use?","risk":"none"}"#;
    let model = Counting::new(&[ask, DONE]);
    let person = Person::new(
        &page,
        vec![(None, Reply::Now(HumanReply::Answer("Documents".into())))],
    );
    let (stop, sink) = (flag(), Seen::default());
    let trace = engine(&model, &page, &hands, &stop, Some(&person), &sink)
        .run("save the report")
        .await;

    assert_eq!(trace.outcome, Some(Outcome::Success));
    let asked = person.asked.lock().unwrap().clone();
    assert_eq!(asked[0].kind, CheckKind::Question);
    assert_eq!(asked[0].prompt, "Which folder should I use?");
    assert!(
        model.seen()[1]
            .text
            .contains("The user answered your question: Documents")
    );
    assert_eq!(trace.human[0].result, "answered");
    // The answer itself stays out of the trace.
    assert!(
        !serde_json::to_string(&trace.human)
            .unwrap()
            .contains("Documents")
    );
}

#[tokio::test]
async fn questions_are_bounded() {
    let page = Page::at(PLAIN);
    let hands = Hands::new(&page);
    let ask = r#"{"thought":"?","action":"ask_user","text":"Again?","risk":"none"}"#;
    let model = Counting::new(&[ask; 10]);
    let script = (0..10)
        .map(|_| (None, Reply::Now(HumanReply::Answer("yes".into()))))
        .collect();
    let person = Person::new(&page, script);
    let (stop, sink) = (flag(), Seen::default());
    let trace = engine(&model, &page, &hands, &stop, Some(&person), &sink)
        .run("loop")
        .await;

    assert_eq!(trace.outcome, Some(Outcome::AskedUser));
    assert_eq!(person.kinds().len(), kai_core::human::MAX_ASKS as usize);
    assert_eq!(trace.human.last().unwrap().result, "too_many_asks");
}

#[tokio::test]
async fn the_kill_switch_works_while_waiting() {
    let page = Page::at(CAPTCHA);
    let hands = Hands::new(&page);
    let model = Counting::new(&[DONE]);
    let stop = flag();
    let mut person = Person::new(&page, vec![(None, Reply::Never)]);
    person.stop = Some(&stop);
    let sink = Seen::default();
    let trace = tokio::time::timeout(
        Duration::from_secs(5),
        engine(&model, &page, &hands, &stop, Some(&person), &sink).run("log in"),
    )
    .await
    .expect("the stop was not seen while waiting");
    assert_eq!(trace.outcome, Some(Outcome::Stopped));
    assert_eq!(trace.human[0].result, "stopped");
}

#[tokio::test]
async fn nobody_answering_times_out() {
    let page = Page::at(CAPTCHA);
    let hands = Hands::new(&page);
    let model = Counting::new(&[DONE]);
    let person = Person::new(&page, vec![(None, Reply::Never)]);
    let (stop, sink) = (flag(), Seen::default());
    let mut e = engine(&model, &page, &hands, &stop, Some(&person), &sink);
    e.config.human_wait = Duration::from_millis(50);
    let trace = e.run("log in").await;
    assert_eq!(trace.outcome, Some(Outcome::AskedUser));
    assert_eq!(trace.human[0].result, "timed_out");
    assert_eq!(
        kai_core::failure::classify(&trace, None).unwrap().category,
        Category::NeedsHuman
    );
}

#[tokio::test]
async fn waiting_for_the_person_is_not_kais_time() {
    let page = Page::at(CAPTCHA);
    let hands = Hands::new(&page);
    let model = Counting::new(&[DONE]);
    let mut person = Person::new(
        &page,
        vec![(Some(WELCOME), Reply::Now(HumanReply::Resolved))],
    );
    person.delay = Duration::from_millis(400);
    let (stop, sink) = (flag(), Seen::default());
    let mut e = engine(&model, &page, &hands, &stop, Some(&person), &sink);
    e.config.max_time = Duration::from_millis(200);
    let trace = e.run("log in").await;
    assert_eq!(trace.outcome, Some(Outcome::Success), "{:?}", trace.human);
    assert!(trace.human[0].waited_ms >= 400);
}

#[tokio::test]
async fn certain_follow_ups_never_touch_a_captcha_either() {
    let page = Page::at(CAPTCHA);
    let hands = Hands::new(&page);
    // Submit, then (no model call) tick the box: the tick must not run.
    let batch = r#"{"thought":"submit","action":"click","target":2,"risk":"low",
        "then":[{"action":"click","target":1}]}"#;
    let model = Counting::new(&[batch, DONE]);
    let person = Person::new(&page, vec![(None, Reply::Now(HumanReply::Resolved))]);
    let (stop, sink) = (flag(), Seen::default());
    let trace = engine(&model, &page, &hands, &stop, Some(&person), &sink)
        .run("log in")
        .await;
    assert_eq!(hands.log(), vec!["Click Some(2) ".to_string()]);
    assert_eq!(trace.outcome, Some(Outcome::Success));
}
