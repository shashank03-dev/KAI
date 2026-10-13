//! Tree-only steps, the cheap verifier, the zoomed crop after a missed grid
//! click, and the plan cache. Observations and model calls are counted.

mod common;
use async_trait::async_trait;
use common::*;
use kai_core::engine::{CACHED, Verifier};
use kai_core::*;
use std::sync::atomic::Ordering;

const CLICK_SAVE: &str =
    r#"{"thought":"x","action":"click","target":3,"risk":"none","expect":"file saved"}"#;
const CLICK_HOME: &str =
    r#"{"thought":"x","action":"click","target":9,"risk":"none","expect":"home page"}"#;
const DONE: &str = r#"{"thought":"done","action":"done","risk":"none"}"#;

#[tokio::test]
async fn tree_only_steps_skip_the_screenshot_with_a_full_look_every_third() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[
        CLICK_SAVE, CLICK_HOME, CLICK_SAVE, CLICK_HOME, CLICK_SAVE, DONE,
    ]);
    let trace = engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    let images: Vec<bool> = client.seen().iter().map(|s| s.had_image).collect();
    assert_eq!(images, vec![true, false, false, true, false, false]);
    assert!(client.seen()[1].text.contains("(No screenshot this step.)"));
    eprintln!(
        "MEASURED 6 model calls: {} with screenshot, {} tree-only",
        images.iter().filter(|b| **b).count(),
        images.iter().filter(|b| !**b).count()
    );

    // A/B switch: tree_only off sends a screenshot every step.
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let client = Counting::new(&[CLICK_SAVE, CLICK_HOME, DONE]);
    let mut e = engine(&client, &screen, &hands, &ap, &stop);
    e.config.tree_only = false;
    e.run("x").await;
    assert!(client.seen().iter().all(|s| s.had_image));
    assert_eq!(screen.tree.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn need_screen_and_visual_expectations_force_a_full_look() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[
        CLICK_SAVE,
        r#"{"thought":"x","action":"wait","risk":"none","need_screen":true}"#,
        r#"{"thought":"x","action":"click","target":9,"risk":"none","expect":"the photo is rotated"}"#,
        DONE,
    ]);
    engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    let images: Vec<bool> = client.seen().iter().map(|s| s.had_image).collect();
    // full, tree-only (after a checkable click), full (need_screen), full (visual)
    assert_eq!(images, vec![true, false, true, true]);
}

#[tokio::test]
async fn no_progress_means_a_full_look_next() {
    let screen = Screen::marks();
    let mut hands = Hands::new(&screen);
    hands.frozen = true;
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[CLICK_SAVE, CLICK_SAVE, DONE]);
    let trace = engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    let images: Vec<bool> = client.seen().iter().map(|s| s.had_image).collect();
    assert_eq!(images, vec![true, false, true]);
    assert!(!trace.steps[0].verified, "nothing changed after the click");
}

#[tokio::test]
async fn a_silent_shell_step_is_not_reported_as_a_stalled_screen() {
    // A command that writes a file changes no pixels. The model must not be
    // told "the screen has not changed, try a different approach": it then
    // throws away a command that worked (seen live: a screenshot task took
    // 40 steps).
    let shell = r#"{"thought":"save it","action":"shell","text":"import -window root ~/s.png","risk":"low","expect":"saved"}"#;
    let screen = Screen::marks();
    let mut hands = Hands::new(&screen);
    hands.frozen = true;
    let (ap, stop) = (approve_all(), flag());
    let other = r#"{"thought":"check","action":"shell","text":"ls ~/s.png","risk":"low","expect":"listed"}"#;
    let client = Counting::new(&[shell, other, DONE]);
    let trace = engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    for seen in client.seen() {
        assert!(!seen.text.contains("has not changed"), "{}", seen.text);
    }
}

#[tokio::test]
async fn the_same_shell_command_twice_is_called_out_as_a_loop() {
    // Seen live: Gemma wrote the same file 40 times once nothing said stop.
    let echo = r#"{"thought":"write it","action":"shell","text":"echo 101.52 > ~/tip.txt","risk":"low","expect":"written"}"#;
    let screen = Screen::marks();
    let mut hands = Hands::new(&screen);
    hands.frozen = true;
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[echo, echo, DONE]);
    engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    let third = &client.seen()[2].text;
    assert!(third.contains("same command twice"), "{third}");
    assert!(third.contains("has not changed"), "{third}");
}

#[tokio::test]
async fn a_model_that_keeps_asking_for_a_refused_repeat_is_stopped() {
    // A refused repeat prints a note, which used to count as terminal
    // progress: the run went on until its step limit.
    let echo = r#"{"thought":"write it","action":"shell","text":"echo 101.52 > ~/tip.txt","risk":"low","expect":"written"}"#;
    let screen = Screen::marks();
    let mut hands = Hands::new(&screen);
    hands.frozen = true;
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[echo; 30]);
    let mut e = engine(&client, &screen, &hands, &ap, &stop);
    e.config.max_steps = 30;
    let trace = e.run("x").await;
    assert_eq!(trace.outcome, Some(Outcome::AskedUser));
    assert!(client.count() <= 8, "{} model calls", client.count());
}

struct FixedVerifier(f32);
#[async_trait]
impl Verifier for FixedVerifier {
    async fn check(&self, _task: &str, expect: &str, state: &str) -> Option<f32> {
        assert!(!expect.is_empty());
        assert!(state.contains("Elements now:") && state.contains("#3 button"));
        Some(self.0)
    }
}

#[tokio::test]
async fn verifier_flag_forces_screenshot_and_the_careful_model() {
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let (ap, stop) = (approve_all(), flag());
    let fast = Counting::new(&[CLICK_SAVE]);
    let mut careful = Counting::new(&[DONE]);
    careful.backend = Backend::Gemini;
    let v = FixedVerifier(0.05);
    let mut e = engine(&fast, &screen, &hands, &ap, &stop);
    e.careful = Some(&careful);
    e.verifier = Some(&v);
    let trace = e.run("x").await;
    assert_eq!(trace.outcome, Some(Outcome::Success));
    assert_eq!(fast.count(), 1);
    assert_eq!(careful.count(), 1, "flagged step goes to the careful model");
    let s = &careful.seen()[0];
    assert!(s.had_image, "flagged step gets a screenshot");
    assert!(s.text.contains("does not seem to hold"));
    assert!(!trace.steps[0].verified);
}

#[tokio::test]
async fn verifier_confirms_a_batched_step() {
    let screen = Screen::marks();
    let mut hands = Hands::new(&screen);
    hands.frozen = true; // pixels unchanged, but the verifier reads the tree
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[
        r#"{"thought":"x","action":"key","keys":["ctrl","a"],"risk":"none","expect":"all selected","then":[{"action":"key","keys":["ctrl","c"],"expect":"copied"}]}"#,
        DONE,
    ]);
    let v = FixedVerifier(0.95);
    let mut e = engine(&client, &screen, &hands, &ap, &stop);
    e.verifier = Some(&v);
    let trace = e.run("copy everything").await;
    assert!(trace.steps[0].verified && trace.steps[1].verified);
    assert!(trace.steps[2].timings.verify_ms < 1000);
}

#[tokio::test]
async fn missed_grid_click_sends_a_zoomed_crop() {
    let screen = Screen::grid();
    let mut hands = Hands::new(&screen);
    hands.frozen = true;
    let (ap, stop) = (approve_all(), flag());
    let client = Counting::new(&[
        r#"{"thought":"x","action":"click","target":6,"risk":"none","expect":"opens"}"#,
        DONE,
    ]);
    engine(&client, &screen, &hands, &ap, &stop).run("x").await;
    let seen = client.seen();
    assert!(!seen[0].had_crop);
    assert!(
        seen[1].had_crop && seen[1].had_image,
        "grid never goes tree-only"
    );
    assert!(seen[1].text.contains("click on cell 6 changed nothing"));
    assert_eq!(screen.zooms.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn plan_cache_replays_the_deterministic_prefix() {
    let dir = std::env::temp_dir().join(format!("kai-fast-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = PlanCache::load(&dir).unwrap();
    let opts = RunOptions {
        plan_cache: Some(&cache),
        ..Default::default()
    };
    let script = [
        r#"{"thought":"x","action":"open_url","text":"https://example.com","risk":"none"}"#,
        r#"{"thought":"x","action":"shell","text":"echo hi","risk":"none"}"#,
        r#"{"thought":"x","action":"key","keys":["ctrl","l"],"risk":"none"}"#,
        CLICK_SAVE,
        DONE,
    ];
    let (ap, stop) = (approve_all(), flag());

    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let first = Counting::new(&script);
    let mut e = engine(&first, &screen, &hands, &ap, &stop);
    e.config.max_batch = 0;
    let t1 = e.run_with("Open example.com and save", opts).await;
    assert_eq!(t1.outcome, Some(Outcome::Success));
    assert_eq!(first.count(), 5);
    assert_eq!(cache.len(), 1);

    // Same task, phrased a little differently: the prefix replays.
    let screen = Screen::marks();
    let hands = Hands::new(&screen);
    let second = Counting::new(&[CLICK_SAVE, DONE]);
    let cache2 = PlanCache::load(&dir).unwrap();
    let e = engine(&second, &screen, &hands, &ap, &stop);
    let t2 = e
        .run_with(
            "please open example.com and save",
            RunOptions {
                plan_cache: Some(&cache2),
                ..Default::default()
            },
        )
        .await;
    assert_eq!(t2.outcome, Some(Outcome::Success));
    assert_eq!(second.count(), 2);
    assert_eq!(t2.steps[0].backend, CACHED);
    assert_eq!(&hands.log()[..3], &t1_log());
    assert!(second.seen()[0].had_image, "verified by a full observation");
    assert!(second.seen()[0].text.contains("remembered plan"));
    eprintln!("MEASURED cached task: 5 model calls cold, 2 with the plan cache");
    let _ = std::fs::remove_dir_all(&dir);
}

fn t1_log() -> [String; 3] {
    [
        "open_url https://example.com".into(),
        "shell echo hi".into(),
        "Key None [\"ctrl\", \"l\"]".into(),
    ]
}
