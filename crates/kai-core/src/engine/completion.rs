//! Independent, read-only completion evidence for narrowly identified tasks.
//! No model can extend the run after a successful check, including a queued
//! computer action or a follow-up in the same model reply.

use super::{Engine, Run};
use crate::{Grounding, Observation, Outcome, Role};
use std::time::Duration;
use url::Url;

const SAMPLE_GAP: Duration = Duration::from_millis(250);
const READ_LIMIT: Duration = Duration::from_secs(2);
// Appended by kai-app for GUI-only runs. Accept this one known suffix only;
// otherwise a second line can be another task that must still be completed.
const GUI_ONLY_SUFFIX: &str = "\nUse visible GUI controls only. Never open a terminal or execute shell commands, scripts, or code. Ask the user if blocked.";

/// Require one *named* media goal, not a compound instruction or an arbitrary
/// request containing the word "play" (which can mean many things).
pub(super) fn media_goal(task: &str) -> Option<String> {
    let task = task
        .trim()
        .strip_suffix(GUI_ONLY_SUFFIX)
        .unwrap_or(task)
        .trim();
    if task.contains(['\n', '\r']) {
        return None;
    }
    let task = task.to_lowercase();
    let (prefix, tail) = task.split_once("play ")?;
    let prefix = prefix.trim();
    let prefix = prefix
        .strip_prefix("please ")
        .or_else(|| (prefix == "please").then_some(""))
        .unwrap_or(prefix);
    if !prefix.is_empty() && !browser_open_prefix(prefix) {
        return None;
    }
    // Only final sentence punctuation is allowed. An interior separator can
    // introduce another task, which playback alone would not establish.
    let mut title = tail.trim().trim_end_matches(['.', '!', '?']).trim();
    if title.contains([';', '&', ',', '.', '!', '?', '\n', '\r'])
        || [
            " plus ",
            " as well as ",
            " along with ",
            " while ",
            " followed by ",
        ]
        .iter()
        .any(|connector| title.contains(connector))
    {
        return None;
    }
    for suffix in [" on youtube music", " on youtube", " in youtube"] {
        if let Some(rest) = title.strip_suffix(suffix) {
            title = rest;
        }
    }
    let mut explicit_media = false;
    for suffix in [" music video", " song", " video", " track"] {
        if let Some(rest) = title.strip_suffix(suffix) {
            title = rest;
            explicit_media = true;
            break;
        }
    }
    if !explicit_media && !tail.contains("youtube") {
        return None;
    }
    title = title
        .trim_start_matches("the ")
        .trim_matches(['\'', '"', ' ']);
    let tokens = words(title);
    if tokens.is_empty()
        || tokens.len() > 9
        || (tokens.len() <= 2
            && tokens.iter().all(|s| {
                matches!(
                    s.as_str(),
                    "a" | "any"
                        | "some"
                        | "the"
                        | "music"
                        | "song"
                        | "songs"
                        | "video"
                        | "videos"
                        | "track"
                        | "tracks"
                        | "something"
                        | "anything"
                )
            }))
        || (tokens.len() == 1
            && matches!(
                tokens[0].as_str(),
                "a" | "any" | "some" | "the" | "music" | "song"
            ))
        || tokens
            .iter()
            .any(|s| matches!(s.as_str(), "then" | "also" | "after" | "before" | "another"))
        || title.contains(" and ")
        || title.contains(',')
    {
        return None;
    }
    Some(tokens.join(" "))
}

/// Only an optional browser launch can precede playback. Reject other work
/// before the song even when the instruction ends with "and play ...".
fn browser_open_prefix(prefix: &str) -> bool {
    let Some(open) = prefix
        .strip_prefix("open ")
        .or_else(|| prefix.strip_prefix("launch "))
    else {
        return false;
    };
    let Some(browser) = open.strip_suffix(" browser and") else {
        return false;
    };
    let browser = browser.trim();
    matches!(
        browser,
        "zen"
            | "chrome"
            | "google chrome"
            | "chromium"
            | "firefox"
            | "safari"
            | "brave"
            | "edge"
            | "microsoft edge"
            | "arc"
            | "opera"
            | "vivaldi"
    )
}

/// An explicit launch is a separately required outcome. The active window
/// must independently identify the requested browser for an automatic stop.
fn requested_browser(task: &str) -> Option<&str> {
    let task = task.trim();
    let task = task.strip_suffix(GUI_ONLY_SUFFIX).unwrap_or(task).trim();
    let lower = task.to_ascii_lowercase();
    let (prefix, _) = lower.split_once("play ")?;
    let prefix = prefix
        .trim()
        .strip_prefix("please ")
        .unwrap_or(prefix.trim());
    let open = prefix
        .strip_prefix("open ")
        .or_else(|| prefix.strip_prefix("launch "))?;
    // Return the slice from the original task only via its lowercased form is
    // impossible; callers compare the normalized browser name directly.
    let browser = open.strip_suffix(" browser and")?;
    if browser_open_prefix(prefix) {
        Some(match browser {
            "zen" => "zen",
            "chrome" | "google chrome" => "chrome",
            "chromium" => "chromium",
            "firefox" => "firefox",
            "safari" => "safari",
            "brave" => "brave",
            "edge" | "microsoft edge" => "edge",
            "arc" => "arc",
            "opera" => "opera",
            "vivaldi" => "vivaldi",
            _ => return None,
        })
    } else {
        None
    }
}

fn browser_identity_matches(task: &str, obs: &Observation) -> bool {
    let Some(browser) = requested_browser(task) else {
        return true;
    };
    let Some(window) = obs.window_title.as_deref() else {
        return false;
    };
    let window = window.trim().to_ascii_lowercase();
    [" - ", " — ", " | "]
        .iter()
        .any(|separator| window.ends_with(&format!("{separator}{browser}")))
}

fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn title_matches(goal: &str, title: &str) -> bool {
    let needle = words(goal);
    let title_words = words(title);
    // A related remix, cover, reaction, or teaser is a different recording.
    const VARIANTS: &[&str] = &[
        "remix",
        "cover",
        "karaoke",
        "reaction",
        "instrumental",
        "mashup",
        "slowed",
        "sped",
        "teaser",
        "parody",
        "reprise",
    ];
    !needle.is_empty()
        && !VARIANTS.iter().any(|word| {
            title_words.iter().any(|token| token == word)
                && !needle.iter().any(|token| token == word)
        })
        && title_words
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Desktop browsers often expose YouTube's play/pause button through their
/// accessibility tree, even when the cursor isn't on the video. A screenshot
/// of the requested video or its title alone is never completion evidence.
fn desktop_playing(goal: &str, obs: &Observation) -> bool {
    let Some(title) = obs.window_title.as_deref() else {
        return false;
    };
    if !title.to_ascii_lowercase().contains("youtube") || !title_matches(goal, title) {
        return false;
    }
    if let Some(url) = &obs.url
        && !youtube_watch(url)
    {
        return false;
    }
    let Grounding::Marks(elements) = &obs.grounding else {
        return false;
    };
    let mut pause = false;
    for el in elements {
        let name = el.name.trim().to_ascii_lowercase();
        if name.contains("skip ad")
            || name.contains("advertisement")
            || name.contains("visit advertiser")
            || name.contains("ad 1 of")
            || name.contains("ad 2 of")
        {
            return false;
        }
        if el.role == Role::Button
            && el.enabled
            && (name == "pause"
                || name.starts_with("pause (")
                || name == "pause video"
                || name == "pause keyboard shortcut k")
        {
            pause = true;
        }
    }
    pause
}

fn youtube_watch(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    matches!(url.scheme(), "https" | "http")
        && matches!(
            url.host_str(),
            Some("youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com")
        )
        && url.path() == "/watch"
        && url
            .query_pairs()
            .any(|(key, value)| key == "v" && !value.is_empty())
}

impl Engine<'_> {
    /// Observe again after an action, before executing any precomputed work.
    pub(super) async fn completion_fresh(&self, r: &mut Run<'_>) -> Option<Outcome> {
        let media_candidate = self.config.auto_complete_media && media_goal(r.task).is_some();
        let inferred_candidate = self.config.auto_complete_verified
            && r.ledger.eligible_for_auto_stop()
            && !r.trace.steps.is_empty();
        if !media_candidate && !inferred_candidate {
            return None;
        }
        let obs = tokio::time::timeout(READ_LIMIT, self.perceptor.observe_tree_only())
            .await
            .ok()?
            .ok()?;
        if let Some(outcome) = self.media_complete(r, &obs).await {
            return Some(outcome);
        }
        self.verified_goal_complete(r, &obs).await
    }

    pub(super) async fn media_complete(
        &self,
        r: &mut Run<'_>,
        obs: &Observation,
    ) -> Option<Outcome> {
        let reason = self.media_evidence(r, obs).await?;
        // A media hit must also satisfy every explicitly required outcome;
        // their probes may contain checks that media state cannot establish.
        if !self.media_outcomes_ready(r, obs).await {
            return None;
        }
        if self.drain_steering(r, r.step)
            || !r.trace.steered.is_empty()
            || r.privacy.active()
            || self.interrupt.stopped()
            || r.out_of_time()
        {
            return None;
        }
        r.trace.completion = Some(reason.to_string());
        r.queued.clear();
        log::info!("completion barrier: {reason}; stopping before the next action");
        Some(Outcome::Success)
    }

    /// Independent evidence can also reject a model-issued `done` when the
    /// normal outcome ledger is disabled; it does not mutate the page.
    pub(super) async fn media_evidence(
        &self,
        r: &Run<'_>,
        obs: &Observation,
    ) -> Option<&'static str> {
        if !self.config.auto_complete_media
            || !r.trace.steered.is_empty()
            || r.privacy.active()
            || self.interrupt.stopped()
            || r.out_of_time()
            || !browser_identity_matches(r.task, obs)
        {
            return None;
        }
        let goal = media_goal(r.task)?;
        let reason = if self.perceptor.reads_pages() {
            // The managed Chromium lane can read the actual media clock.
            // Require playback to advance, and reject muted, ended or ad media.
            let a = tokio::time::timeout(READ_LIMIT, self.perceptor.media_playback())
                .await
                .ok()??;
            if !browser_playing(&goal, &a) {
                return None;
            }
            tokio::time::sleep(SAMPLE_GAP).await;
            let b = tokio::time::timeout(READ_LIMIT, self.perceptor.media_playback())
                .await
                .ok()??;
            if !browser_playing(&goal, &b)
                || a.url != b.url
                || a.title != b.title
                || !(0.05..=5.0).contains(&(b.current_time - a.current_time))
            {
                return None;
            }
            "YouTube media playing: read-only media clock advanced"
        } else {
            if !desktop_playing(&goal, obs) {
                return None;
            }
            tokio::time::sleep(SAMPLE_GAP).await;
            let b = tokio::time::timeout(READ_LIMIT, self.perceptor.observe_tree_only())
                .await
                .ok()?
                .ok()?;
            if obs.window_title != b.window_title || !desktop_playing(&goal, &b) {
                return None;
            }
            "YouTube media playing: matching title and Pause control on two reads"
        };
        (!self.interrupt.stopped() && !r.out_of_time()).then_some(reason)
    }
}

fn browser_playing(goal: &str, media: &crate::ports::MediaPlayback) -> bool {
    youtube_watch(&media.url)
        && title_matches(goal, &media.title)
        && media.playing
        && !media.advertisement
        && media.current_time.is_finite()
        && media.current_time >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_single_explicit_song_requests_are_eligible() {
        for (task, expected) in [
            ("play Channa Mereya song", "channa mereya"),
            ("Please play Channa Mereya song!", "channa mereya"),
            (
                "Open Zen browser and play Channa Mereya song",
                "channa mereya",
            ),
            (
                "Please open Zen browser and play Channa Mereya song",
                "channa mereya",
            ),
            (
                "Launch Google Chrome browser and play Channa Mereya track",
                "channa mereya",
            ),
            (
                "Please launch Firefox browser and play Channa Mereya on YouTube",
                "channa mereya",
            ),
            ("play the Shape of You video on YouTube", "shape of you"),
            ("play 'Channa Mereya' song on YouTube", "channa mereya"),
            ("play Channa Mereya on YouTube Music", "channa mereya"),
            (
                "Open Zen browser and play Channa Mereya song\nUse visible GUI controls only. Never open a terminal or execute shell commands, scripts, or code. Ask the user if blocked.",
                "channa mereya",
            ),
        ] {
            assert_eq!(media_goal(task).as_deref(), Some(expected), "task={task}");
        }
    }

    #[test]
    fn reject_ambiguous_or_compound_requests_even_with_playback_language() {
        for task in [
            "open Zen browser",
            "play a game",
            "play a song",
            "play song",
            "play some music on YouTube",
            "play anything on YouTube",
            "play a video on YouTube",
            "play Channa Mereya song and send an email",
            "play Channa Mereya song, then send an email",
            "play Channa Mereya song and another track",
            "play Channa Mereya song & send an email",
            "play Channa Mereya song; send an email",
            "play Channa Mereya song. Send an email",
            "play Channa Mereya song! Send an email",
            "play Channa Mereya song plus send an email",
            "play Channa Mereya song as well as email the link",
            "play Channa Mereya song while opening files",
            "open files and play Channa Mereya song",
            "open Zen browser and send an email and play Channa Mereya song",
            "open Zen browser and delete files and play Channa Mereya song",
            "open Zen browser, then play Channa Mereya song",
            "please open Zen browser, then play Channa Mereya song",
            "play Channa Mereya song\nthen send the link to a friend",
            "play Channa Mereya song\nUse visible GUI controls only. Then send an email.",
        ] {
            assert!(media_goal(task).is_none(), "must fail closed: {task}");
        }
    }

    #[test]
    fn title_match_requires_complete_adjacent_words() {
        assert!(title_matches(
            "channa mereya",
            "Channa Mereya Full Video - YouTube"
        ));
        assert!(!title_matches(
            "channa mereya",
            "Channa Another Mereya - YouTube"
        ));
        assert!(!title_matches("channa mereya", "Channa Mereyad - YouTube"));
        assert!(!title_matches(
            "channa mereya",
            "Channa Mereya remix - YouTube"
        ));
        assert!(!title_matches(
            "channa mereya",
            "Channa Mereya cover - YouTube"
        ));
        assert!(!title_matches(
            "channa mereya",
            "Channa Mereya karaoke - YouTube"
        ));
        assert!(title_matches(
            "channa mereya remix",
            "Channa Mereya remix - YouTube"
        ));
        assert!(!youtube_watch(
            "https://youtube.com.evil.test/watch?v=bzSTpdcs-EI"
        ));
        assert!(!youtube_watch(
            "https://youtube.com/redirect?next=/watch?v=bzSTpdcs-EI"
        ));
        assert!(!youtube_watch("https://youtube.com/watch?list=playlist"));
        assert!(!youtube_watch("javascript://youtube.com/watch?v=x"));
        assert!(youtube_watch(
            "https://www.youtube.com/watch?app=desktop&v=bzSTpdcs-EI"
        ));
        assert!(youtube_watch(
            "https://music.youtube.com/watch?v=bzSTpdcs-EI"
        ));
        assert!(!youtube_watch(
            "https://music.youtube.com/playlist?list=abc"
        ));
    }
}
