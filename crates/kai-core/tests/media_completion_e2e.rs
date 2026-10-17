//! Regression: verified music playback must end a run before any extra click.

use async_trait::async_trait;
use kai_core::computer::{
    Capabilities, ComputerAction, ExecutionContext, ObservationSpace, ToolError,
};
use kai_core::engine::{
    Actuator, ApprovalChoice, Approver, Engine, Interrupt, NullSink, Observation, Perceptor,
};
use kai_core::ports::MediaPlayback;
use kai_core::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// Real Sony Music India upload of Channa Mereya, with an 11-character video ID.
const WATCH_URL: &str = "https://www.youtube.com/watch?v=bzSTpdcs-EI";

struct Screen {
    actions: AtomicUsize,
    reads: AtomicUsize,
    post_action_media_reads: AtomicUsize,
    tree_reads_after_action: AtomicUsize,
    pre_action_tree_reads: AtomicUsize,
    initially_playing: bool,
    title: &'static str,
    controls: bool,
    ad: bool,
    browser: bool,
    paused: bool,
    clock_moves: bool,
    clock_jumps: bool,
    pause_flickers: bool,
    media_unavailable: bool,
    media_audible: bool,
    media_url_changes: bool,
    initial_tree_unavailable: bool,
    url: &'static str,
    desktop_url: Option<&'static str>,
}

impl Screen {
    fn playing() -> Self {
        Self {
            actions: AtomicUsize::new(0),
            reads: AtomicUsize::new(0),
            post_action_media_reads: AtomicUsize::new(0),
            tree_reads_after_action: AtomicUsize::new(0),
            pre_action_tree_reads: AtomicUsize::new(0),
            initially_playing: false,
            title: "Channa Mereya Full Video - YouTube - Zen",
            controls: true,
            ad: false,
            browser: false,
            paused: false,
            clock_moves: true,
            clock_jumps: false,
            pause_flickers: false,
            media_unavailable: false,
            media_audible: true,
            media_url_changes: false,
            initial_tree_unavailable: false,
            url: WATCH_URL,
            desktop_url: None,
        }
    }
}

#[async_trait]
impl Perceptor for Screen {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let has_acted = self.actions.load(Ordering::SeqCst) > 0;
        let playing = self.initially_playing || has_acted;
        let post_action_read =
            has_acted.then(|| self.tree_reads_after_action.fetch_add(1, Ordering::SeqCst));
        let mut elements = vec![Element {
            mark: 1,
            role: Role::Button,
            name: "Play".into(),
            bounds: Rect::new(0.0, 0.0, 20.0, 20.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }];
        if playing
            && self.controls
            && !self.paused
            && (!self.pause_flickers || post_action_read.is_some_and(|n| n % 2 == 0))
        {
            // Zen's macOS accessibility description uses this wording.
            elements[0].name = "Pause keyboard shortcut k".into();
        }
        if playing && self.ad {
            elements.push(Element {
                mark: 2,
                role: Role::Button,
                name: "Skip Ads".into(),
                bounds: Rect::new(20.0, 0.0, 20.0, 20.0),
                enabled: true,
                secure: false,
                focused: false,
                value: None,
            });
        }
        Ok(Observation {
            images: Images {
                screenshot_jpeg: vec![0xff, 0xd8],
                crop_jpeg: None,
            },
            grounding: Grounding::Marks(elements),
            screen_hash: self.actions.load(Ordering::SeqCst) as u64,
            timing: Default::default(),
            screen: None,
            window_title: playing.then(|| self.title.to_string()),
            url: self
                .desktop_url
                .or(self.browser.then_some(self.url))
                .map(str::to_string),
        })
    }

    async fn observe_tree_only(&self) -> anyhow::Result<Observation> {
        if self.initial_tree_unavailable
            && self.actions.load(Ordering::SeqCst) == 0
            && self.pre_action_tree_reads.fetch_add(1, Ordering::SeqCst) == 0
        {
            anyhow::bail!("initial tree unavailable");
        }
        self.observe().await
    }

    fn reads_pages(&self) -> bool {
        self.browser
    }

    async fn media_playback(&self) -> Option<MediaPlayback> {
        if !self.browser || self.media_unavailable {
            return None;
        }
        if self.actions.load(Ordering::SeqCst) > 0 {
            self.post_action_media_reads.fetch_add(1, Ordering::SeqCst);
        }
        let n = self.reads.fetch_add(1, Ordering::SeqCst);
        Some(MediaPlayback {
            title: self.title.into(),
            url: if self.media_url_changes && n % 2 == 1 {
                "https://www.youtube.com/watch?v=FO9rBogdRRQ"
            } else {
                self.url
            }
            .into(),
            current_time: if self.clock_jumps {
                n as f64 * 10.0
            } else if self.clock_moves {
                n as f64 * 0.25
            } else {
                1.0
            },
            // Browser media inspection maps muted/zero-volume media to playing=false.
            playing: (self.initially_playing || self.actions.load(Ordering::SeqCst) > 0)
                && !self.paused
                && self.media_audible,
            advertisement: self.ad,
        })
    }
}

#[async_trait]
impl Actuator for Screen {
    fn computer_capabilities(&self) -> Capabilities {
        Capabilities::all()
    }
    async fn computer_act(
        &self,
        _: &ComputerAction,
        _: &ObservationSpace,
        _: &Grounding,
        ctx: &ExecutionContext,
    ) -> Result<(), ToolError> {
        ctx.check()?;
        self.actions.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn act(&self, _: &Decision, _: &Grounding) -> anyhow::Result<()> {
        self.actions.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn open_app(&self, _: &str) -> anyhow::Result<()> {
        self.actions.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn open_url(&self, _: &str) -> anyhow::Result<()> {
        self.actions.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn shell(&self, _: &str) -> anyhow::Result<String> {
        self.actions.fetch_add(1, Ordering::SeqCst);
        Ok("ok".into())
    }
}

struct Approve;
#[async_trait]
impl Approver for Approve {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        ApprovalChoice::Approve
    }
}

struct NeverStop;
impl Interrupt for NeverStop {
    fn stopped(&self) -> bool {
        false
    }
}

struct SteerDuringVerification<'a> {
    reads: &'a AtomicUsize,
    fired: AtomicBool,
}

impl kai_core::ports::Steering for SteerDuringVerification<'_> {
    fn take(&self) -> Vec<String> {
        if self.reads.load(Ordering::SeqCst) >= 2 && !self.fired.swap(true, Ordering::SeqCst) {
            vec!["Also send the link".into()]
        } else {
            Vec::new()
        }
    }
}

const REPEATING_CLICK: &str =
    r#"{"thought":"click again","action":"click","target":1,"risk":"low"}"#;
const PREMATURE_DONE: &str =
    r#"{"thought":"the video must be playing","action":"done","risk":"none"}"#;
const BATCH_CLICK: &str = r#"{"thought":"play","action":"click","target":1,"risk":"low","then":[{"action":"key","keys":["space"]},{"action":"key","keys":["enter"]}]}"#;
const COMPUTER_CLICK: &str = r#"{"thought":"start playback","action":"computer","computer":{"type":"click","target":{"kind":"element","observation":1,"mark":1},"button":"left","count":1},"risk":"low"}"#;
const OAI_QUEUED_CLICKS: &str = r#"{"thought":"start and click again","actions":[{"type":"click","button":"left","x":null,"y":null,"mark":1,"keys":null},{"type":"click","button":"left","x":null,"y":null,"mark":1,"keys":null}],"message":null,"phase":"commentary"}"#;

async fn run_configured(screen: &Screen, task: &str, reply: &str, config: Config) -> Trace {
    let client = MockClient::always(reply);
    let engine = Engine {
        client: &client,
        careful: None,
        grounder: None,
        perceptor: screen,
        actuator: screen,
        approver: &Approve,
        interrupt: &NeverStop,
        sink: &NullSink,
        steering: &kai_core::engine::NoSteering,
        verifier: None,
        human: None,
        skills: &[],
        config,
    };
    engine.run(task).await
}

async fn run(screen: &Screen, task: &str, reply: &str) -> Trace {
    run_configured(
        screen,
        task,
        reply,
        Config {
            auto_complete_media: true,
            max_steps: 3,
            ..Config::default()
        },
    )
    .await
}

#[tokio::test]
async fn desktop_video_stops_without_waiting_for_model_done() {
    let screen = Screen::playing();
    let trace = run(
        &screen,
        "Open Zen browser and play Channa Mereya song",
        REPEATING_CLICK,
    )
    .await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 1);
    assert_eq!(trace.steps.len(), 1);
    assert!(trace.completion.as_deref().unwrap().contains("Pause"));
}

#[tokio::test]
async fn already_playing_media_stops_before_any_model_or_computer_action() {
    for browser in [false, true] {
        let mut screen = Screen::playing();
        screen.browser = browser;
        screen.initially_playing = true;
        let trace = run(&screen, "play Channa Mereya song", REPEATING_CLICK).await;
        assert_eq!(trace.outcome, Some(Outcome::Success), "browser={browser}");
        assert_eq!(
            screen.actions.load(Ordering::SeqCst),
            0,
            "browser={browser}"
        );
        assert!(trace.steps.is_empty(), "browser={browser}");
        assert!(trace.completion.is_some(), "browser={browser}");
        if browser {
            assert_eq!(screen.reads.load(Ordering::SeqCst), 2);
            assert_eq!(screen.post_action_media_reads.load(Ordering::SeqCst), 0);
        }
    }
}

#[tokio::test]
async fn model_done_without_playback_proof_is_refused_three_times() {
    for browser in [false, true] {
        let mut screen = Screen::playing();
        screen.browser = browser;
        let trace = run_configured(
            &screen,
            "play Channa Mereya song",
            PREMATURE_DONE,
            Config {
                auto_complete_media: true,
                max_steps: 6,
                ..Config::default()
            },
        )
        .await;
        assert_eq!(trace.outcome, Some(Outcome::AskedUser), "browser={browser}");
        // Three refused model actions plus the human-question step.
        assert_eq!(trace.steps.len(), 4, "browser={browser}");
        assert_eq!(
            trace
                .steps
                .iter()
                .filter(|s| s.path.as_deref() == Some("gate") && !s.verified)
                .count(),
            3,
            "browser={browser}"
        );
        assert_eq!(trace.steps[3].path.as_deref(), Some("human"));
        assert_eq!(trace.human.len(), 1);
        assert_eq!(
            screen.actions.load(Ordering::SeqCst),
            0,
            "browser={browser}"
        );
        assert!(trace.completion.is_none(), "browser={browser}");
    }
}

#[tokio::test]
async fn polite_browser_launch_and_gui_only_requests_stop_after_one_action() {
    for task in [
        "Please play Channa Mereya song",
        "Please open Zen browser and play Channa Mereya song",
        "Launch Google Chrome browser and play Channa Mereya track",
        "Open Zen browser and play Channa Mereya song\nUse visible GUI controls only. Never open a terminal or execute shell commands, scripts, or code. Ask the user if blocked.",
    ] {
        let mut screen = Screen::playing();
        if task.contains("Google Chrome") {
            screen.title = "Channa Mereya Full Video - YouTube - Chrome";
        }
        let trace = run(&screen, task, REPEATING_CLICK).await;
        assert_eq!(trace.outcome, Some(Outcome::Success), "task={task}");
        assert_eq!(screen.actions.load(Ordering::SeqCst), 1, "task={task}");
        assert_eq!(trace.steps.len(), 1, "task={task}");
        assert!(trace.completion.is_some(), "task={task}");
    }
}

#[tokio::test]
async fn computer_tool_completion_wins_at_the_last_allowed_step() {
    let screen = Screen::playing();
    let trace = run_configured(
        &screen,
        "Open Zen browser and play Channa Mereya song",
        COMPUTER_CLICK,
        Config {
            auto_complete_media: true,
            computer_tools_v1: true,
            max_steps: 1,
            ..Config::default()
        },
    )
    .await;
    assert_eq!(screen.actions.load(Ordering::SeqCst), 1);
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(trace.steps.len(), 1);
}

#[tokio::test]
async fn provider_queued_computer_clicks_stop_after_verified_playback() {
    let screen = Screen::playing();
    let trace = run_configured(
        &screen,
        "Play Channa Mereya song",
        OAI_QUEUED_CLICKS,
        Config {
            auto_complete_media: true,
            computer_tools_v1: true,
            oai_contract: Some(kai_core::oai_contract::CoordFrame::Marks),
            oai_batch: 2,
            max_steps: 2,
            ..Config::default()
        },
    )
    .await;
    assert_eq!(screen.actions.load(Ordering::SeqCst), 1);
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(trace.steps.len(), 1);
}

#[tokio::test]
async fn successful_playback_drops_unexecuted_batch() {
    let screen = Screen::playing();
    let trace = run(&screen, "Play Channa Mereya song", BATCH_CLICK).await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(
        screen.actions.load(Ordering::SeqCst),
        1,
        "batch must not press keys"
    );
    assert_eq!(trace.steps.len(), 1);
}

#[tokio::test]
async fn browser_requires_advancing_unmuted_non_ad_media() {
    let mut screen = Screen::playing();
    screen.browser = true;
    let trace = run(&screen, "play Channa Mereya song", REPEATING_CLICK).await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 1);
    assert!(trace.completion.as_deref().unwrap().contains("clock"));
    // The intentional initial probe consumes one read before the first click.
    // Verified post-action playback must still require two fresh clock samples.
    assert_eq!(screen.reads.load(Ordering::SeqCst), 3);
    assert_eq!(screen.post_action_media_reads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn media_completion_is_opt_in_even_when_playback_is_verified() {
    for browser in [false, true] {
        let mut screen = Screen::playing();
        screen.browser = browser;
        let trace = run_configured(
            &screen,
            "play Channa Mereya song",
            REPEATING_CLICK,
            Config {
                max_steps: 2,
                ..Config::default()
            },
        )
        .await;
        assert_eq!(trace.outcome, Some(Outcome::StepLimit), "browser={browser}");
        assert_eq!(trace.steps.len(), 2, "browser={browser}");
        assert_eq!(
            screen.actions.load(Ordering::SeqCst),
            2,
            "browser={browser}"
        );
        assert!(trace.completion.is_none(), "browser={browser}");
        assert_eq!(screen.reads.load(Ordering::SeqCst), 0, "browser={browser}");
    }
}

#[tokio::test]
async fn false_positive_signals_never_stop_the_run() {
    for variant in 0..17 {
        let mut screen = Screen::playing();
        match variant {
            0 => screen.title = "A different song - YouTube",
            1 => screen.controls = false,
            2 => screen.paused = true,
            3 => screen.ad = true,
            4 => {
                screen.browser = true;
                screen.clock_moves = false;
            }
            5 => {
                screen.browser = true;
                screen.paused = true;
            }
            6 => {
                screen.browser = true;
                screen.ad = true;
            }
            7 => screen.pause_flickers = true,
            8 => {
                screen.browser = true;
                screen.title = "A different song - YouTube";
            }
            9 => {
                screen.browser = true;
                screen.url = "https://youtube.com.evil.test/watch?v=bzSTpdcs-EI";
            }
            10 => {
                screen.browser = true;
                screen.url = "https://www.youtube.com/results?search_query=Channa+Mereya";
            }
            11 => {
                screen.desktop_url = Some("https://youtube.com.evil.test/watch?v=bzSTpdcs-EI");
            }
            12 => {
                screen.browser = true;
                screen.media_unavailable = true;
            }
            13 => {
                screen.browser = true;
                screen.media_audible = false;
            }
            14 => {
                screen.browser = true;
                screen.clock_jumps = true;
            }
            15 => {
                screen.browser = true;
                screen.media_url_changes = true;
            }
            16 => screen.title = "Channa Mereya Full Video - Other Site",
            _ => unreachable!(),
        }
        let trace = run_configured(
            &screen,
            "play Channa Mereya song",
            REPEATING_CLICK,
            Config {
                auto_complete_media: true,
                max_steps: 2,
                ..Config::default()
            },
        )
        .await;
        assert_eq!(trace.outcome, Some(Outcome::StepLimit), "variant {variant}");
        assert_eq!(trace.steps.len(), 2, "variant {variant}");
        assert_eq!(
            screen.actions.load(Ordering::SeqCst),
            2,
            "variant {variant}"
        );
        assert!(trace.completion.is_none(), "variant {variant}");
    }
}

#[tokio::test]
async fn compound_tasks_do_not_automatically_complete() {
    for task in [
        "play Channa Mereya song and email the link",
        "open my files and play Channa Mereya song",
        "Open Zen browser and send an email and play Channa Mereya song",
        "play Channa Mereya song\nthen send the link",
        "play Channa Mereya song\nUse visible GUI controls only. Then send an email.",
        "play Channa Mereya song & send an email",
        "play Channa Mereya song. Send an email",
        "play Channa Mereya song; send an email",
        "play Channa Mereya song plus send an email",
    ] {
        let screen = Screen::playing();
        let trace = run_configured(
            &screen,
            task,
            REPEATING_CLICK,
            Config {
                auto_complete_media: true,
                max_steps: 2,
                ..Config::default()
            },
        )
        .await;
        assert_eq!(trace.outcome, Some(Outcome::StepLimit), "task={task}");
        assert_eq!(trace.steps.len(), 2, "task={task}");
        assert_eq!(screen.actions.load(Ordering::SeqCst), 2, "task={task}");
        assert!(trace.completion.is_none(), "task={task}");
    }
}

#[tokio::test]
async fn requested_browser_and_recording_must_match() {
    for (task, title) in [
        (
            "Open Zen browser and play Channa Mereya song",
            "Channa Mereya - YouTube - Chrome",
        ),
        (
            "Launch Google Chrome browser and play Channa Mereya song",
            "Channa Mereya - YouTube - Zen",
        ),
        (
            "play Channa Mereya song",
            "Channa Mereya remix - YouTube - Zen",
        ),
        (
            "play Channa Mereya song",
            "Channa Mereya cover - YouTube - Zen",
        ),
    ] {
        let mut screen = Screen::playing();
        screen.title = title;
        let trace = run_configured(
            &screen,
            task,
            REPEATING_CLICK,
            Config {
                auto_complete_media: true,
                max_steps: 2,
                ..Config::default()
            },
        )
        .await;
        assert_eq!(
            trace.outcome,
            Some(Outcome::StepLimit),
            "task={task}, title={title}"
        );
        assert_eq!(screen.actions.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn youtube_music_watch_page_supports_clock_verification() {
    let mut screen = Screen::playing();
    screen.browser = true;
    screen.url = "https://music.youtube.com/watch?v=bzSTpdcs-EI";
    screen.title = "Channa Mereya - YouTube Music";
    let trace = run(
        &screen,
        "play Channa Mereya on YouTube Music",
        REPEATING_CLICK,
    )
    .await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 1);
    assert_eq!(screen.post_action_media_reads.load(Ordering::SeqCst), 2);
}

/// The OS verifier sees a task-derived file appear after the first action.
/// The mock model deliberately keeps clicking instead of claiming `done`.
struct FileResult<'a> {
    actions: &'a AtomicUsize,
    already_exists: bool,
    never_exists: bool,
}

#[async_trait]
impl kai_core::Probe for FileResult<'_> {
    async fn check(
        &self,
        probe: &kai_core::probe::ProbeSpec,
    ) -> anyhow::Result<kai_core::probe::ProbeVerdict> {
        assert!(matches!(
            probe,
            kai_core::probe::ProbeSpec::FileNotEmpty { .. }
        ));
        let exists =
            !self.never_exists && (self.already_exists || self.actions.load(Ordering::SeqCst) > 0);
        Ok(if exists {
            kai_core::probe::ProbeVerdict::pass("verified output")
        } else {
            kai_core::probe::ProbeVerdict::fail("output missing")
        })
    }
}

async fn run_file_goal(
    screen: &Screen,
    task: &str,
    reply: &str,
    probe: Option<&dyn kai_core::Probe>,
) -> Trace {
    let client = MockClient::always(reply);
    let engine = Engine {
        client: &client,
        careful: None,
        grounder: None,
        perceptor: screen,
        actuator: screen,
        approver: &Approve,
        interrupt: &NeverStop,
        sink: &NullSink,
        steering: &kai_core::engine::NoSteering,
        verifier: None,
        human: None,
        skills: &[],
        config: Config {
            auto_complete_verified: true,
            outcomes: true,
            task_checks: true,
            max_steps: 3,
            ..Config::default()
        },
    };
    engine
        .run_with(
            task,
            kai_core::engine::RunOptions {
                probe,
                ..Default::default()
            },
        )
        .await
}

const FILE_TASK: &str = "Write notes into /tmp/kai-completion-regression.txt";

#[tokio::test]
async fn verified_file_goal_stops_repeated_clicks_after_one_action() {
    let screen = Screen::playing();
    let probe = FileResult {
        actions: &screen.actions,
        already_exists: false,
        never_exists: false,
    };
    let trace = run_file_goal(&screen, FILE_TASK, REPEATING_CLICK, Some(&probe)).await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 1);
    assert_eq!(trace.steps.len(), 1);
    assert!(
        trace
            .completion
            .as_deref()
            .unwrap()
            .contains("task-derived")
    );
}

#[tokio::test]
async fn verified_file_goal_cancels_follow_up_batch_actions() {
    let screen = Screen::playing();
    let probe = FileResult {
        actions: &screen.actions,
        already_exists: false,
        never_exists: false,
    };
    let trace = run_file_goal(&screen, FILE_TASK, BATCH_CLICK, Some(&probe)).await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 1);
    assert_eq!(trace.steps.len(), 1);
}

#[tokio::test]
async fn preexisting_file_does_not_trigger_new_success() {
    let screen = Screen::playing();
    let probe = FileResult {
        actions: &screen.actions,
        already_exists: true,
        never_exists: false,
    };
    let trace = run_file_goal(&screen, FILE_TASK, REPEATING_CLICK, Some(&probe)).await;
    assert_eq!(trace.outcome, Some(Outcome::StepLimit));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 3);
    assert!(trace.completion.is_none());
}

#[tokio::test]
async fn failed_or_unavailable_probe_never_auto_stops() {
    for provide_probe in [true, false] {
        let screen = Screen::playing();
        let probe = FileResult {
            actions: &screen.actions,
            already_exists: false,
            never_exists: true,
        };
        let trace = run_file_goal(
            &screen,
            FILE_TASK,
            REPEATING_CLICK,
            provide_probe.then_some(&probe as &dyn kai_core::Probe),
        )
        .await;
        assert_eq!(trace.outcome, Some(Outcome::StepLimit));
        assert_eq!(screen.actions.load(Ordering::SeqCst), 3);
        assert!(trace.completion.is_none());
    }
}

#[tokio::test]
async fn weak_model_outcome_cannot_authorize_auto_stop() {
    let screen = Screen::playing();
    let probe = FileResult {
        actions: &screen.actions,
        already_exists: false,
        never_exists: false,
    };
    let reply = r#"{"thought":"click again","action":"click","target":1,"risk":"low","outcomes":[{"id":"unverified","description":"I also finished the job"}]}"#;
    let trace = run_file_goal(&screen, FILE_TASK, reply, Some(&probe)).await;
    assert_eq!(trace.outcome, Some(Outcome::StepLimit));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 3);
    assert!(trace.completion.is_none());
}

#[tokio::test]
async fn multi_goal_request_cannot_auto_stop_from_one_file_probe() {
    let screen = Screen::playing();
    let probe = FileResult {
        actions: &screen.actions,
        already_exists: false,
        never_exists: false,
    };
    let trace = run_file_goal(
        &screen,
        "Write notes into /tmp/kai-completion-regression.txt and email the result",
        REPEATING_CLICK,
        Some(&probe),
    )
    .await;
    assert_eq!(trace.outcome, Some(Outcome::StepLimit));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 3);
    assert!(trace.completion.is_none());
}

#[tokio::test]
async fn compound_file_goals_with_plus_and_punctuation_do_not_stop_early() {
    for separator in [" plus ", ". ", "! ", "? ", " as well as "] {
        let screen = Screen::playing();
        let probe = FileResult {
            actions: &screen.actions,
            already_exists: false,
            never_exists: false,
        };
        let task = format!("{FILE_TASK}{separator}email the result");
        let trace = run_file_goal(&screen, &task, REPEATING_CLICK, Some(&probe)).await;
        assert_eq!(trace.outcome, Some(Outcome::StepLimit), "task={task}");
        assert_eq!(screen.actions.load(Ordering::SeqCst), 3);
    }
}

#[tokio::test]
async fn failed_initial_baseline_cannot_be_recaptured_after_first_action() {
    let mut screen = Screen::playing();
    screen.initial_tree_unavailable = true;
    let probe = FileResult {
        actions: &screen.actions,
        already_exists: false,
        never_exists: false,
    };
    let trace = run_file_goal(&screen, FILE_TASK, REPEATING_CLICK, Some(&probe)).await;
    assert_eq!(trace.outcome, Some(Outcome::StepLimit));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 3);
    assert!(trace.completion.is_none());
}

#[tokio::test]
async fn media_with_unprobed_model_outcome_does_not_auto_complete() {
    let mut screen = Screen::playing();
    screen.browser = true;
    let reply = r#"{"thought":"click again","action":"click","target":1,"risk":"low","outcomes":[{"id":"other","description":"Send the link to a friend"}]}"#;
    let trace = run_configured(
        &screen,
        "play Channa Mereya song",
        reply,
        Config {
            auto_complete_media: true,
            outcomes: true,
            max_steps: 2,
            ..Config::default()
        },
    )
    .await;
    assert_eq!(trace.outcome, Some(Outcome::StepLimit));
    assert_eq!(screen.actions.load(Ordering::SeqCst), 2);
    assert!(trace.completion.is_none());
}

#[tokio::test]
async fn steering_arriving_during_media_sampling_prevents_early_stop() {
    let mut screen = Screen::playing();
    screen.browser = true;
    let steering = SteerDuringVerification {
        reads: &screen.post_action_media_reads,
        fired: AtomicBool::new(false),
    };
    let client = MockClient::always(REPEATING_CLICK);
    let engine = Engine {
        client: &client,
        careful: None,
        grounder: None,
        perceptor: &screen,
        actuator: &screen,
        approver: &Approve,
        interrupt: &NeverStop,
        sink: &NullSink,
        steering: &steering,
        verifier: None,
        human: None,
        skills: &[],
        config: Config {
            auto_complete_media: true,
            max_steps: 2,
            ..Config::default()
        },
    };
    let trace = engine.run("play Channa Mereya song").await;
    assert_eq!(trace.outcome, Some(Outcome::StepLimit));
    assert_eq!(trace.steered.len(), 1);
    assert_eq!(screen.actions.load(Ordering::SeqCst), 2);
    assert!(trace.completion.is_none());
}
