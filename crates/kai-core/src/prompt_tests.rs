//! Tests for the prompt builder (`prompt.rs`).

use super::*;
use crate::element::{Element, Rect, Role};
use crate::skill::load_from_str;
use std::path::Path;

fn sample_skills() -> Vec<Skill> {
    let a = load_from_str(
        "---\nname: write-email-gmail\ndescription: Compose and send an email in Gmail. Use to write or send mail.\n---\nbody\n",
        Path::new("/tmp/write-email-gmail"),
    )
    .unwrap();
    let b = load_from_str(
        "---\nname: rename-files-in-folder\ndescription: Rename files in a folder. Use to batch rename files.\n---\nbody\n",
        Path::new("/tmp/rename-files-in-folder"),
    )
    .unwrap();
    vec![a, b]
}

#[test]
fn system_prompt_is_stable_then_skills() {
    let p = system_prompt(&sample_skills());
    assert!(p.starts_with("You are Kai"));
    assert!(p.contains("write-email-gmail"));
}

#[test]
fn user_turn_puts_task_last_and_elements_present() {
    let g = Grounding::Marks(vec![Element {
        mark: 1,
        role: Role::Button,
        name: "Save".into(),
        bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
        enabled: true,
        secure: false,
        focused: false,
        value: None,
    }]);
    let t = user_turn("click save", &g, &[], None, false);
    assert!(t.contains("#1 button \"Save\"\n"));
    assert!(t.trim_end().ends_with("JSON."));
    assert!(t.contains("Task: click save"));
}

#[test]
fn selects_skill_by_keywords() {
    let skills = sample_skills();
    let picked = select_skill("please send an email to my professor", &skills).unwrap();
    assert_eq!(picked.front.name, "write-email-gmail");
}

/// A task that names its own site never runs a skill that opens another
/// one (found live: a contact form on 127.0.0.1 opened Gmail).
#[test]
fn a_task_with_its_own_address_does_not_open_another_site() {
    let gmail = load_from_str(
        "---\nname: write-email-gmail\ndescription: Compose an email in Gmail in the browser with recipient, subject and body. Use when the user asks to write, draft or send an email or mail someone.\nmetadata:\n  kai: '[{\"do\":\"open_url\",\"url\":\"https://mail.google.com/mail/u/0/\"}]'\n---\nbody\n",
        Path::new("/tmp/write-email-gmail"),
    )
    .unwrap();
    assert!(!gmail.steps.is_empty());
    let skills = vec![gmail];
    let form = "Open http://127.0.0.1:8765/contact.html in the browser and send the \
                contact form: name Ravi Kumar, email ravi@example.com";
    assert!(select_skill(form, &skills).is_none());
    // Naming the skill's own site, or only an e-mail address, still fits.
    assert!(select_skill("send an email to bob@uni.edu from my browser", &skills).is_some());
    assert!(select_skill("send an email in the browser on mail.google.com", &skills).is_some());
    // A file name is not a site.
    assert!(
        select_skill(
            "send an email in the browser with notes.md attached",
            &skills
        )
        .is_some()
    );
}

#[test]
fn common_words_alone_do_not_pick_a_skill() {
    let skills = sample_skills();
    assert!(select_skill("use the thing and then the other for me", &skills).is_none());
}

#[test]
fn no_skill_when_nothing_matches() {
    let skills = sample_skills();
    assert!(select_skill("play some jazz", &skills).is_none());
}

#[test]
fn history_window_keeps_last_three() {
    let g = Grounding::Marks(vec![]);
    let hist: Vec<HistoryItem> = (0..5)
        .map(|i| HistoryItem::note(format!("step{i}")))
        .collect();
    let t = user_turn("x", &g, &hist, None, false);
    assert!(t.contains("step2") && t.contains("step4"));
    assert!(!t.contains("step0"));
}

/// A typical browser screen: 40 elements, mostly enabled, a few unnamed.
fn typical_screen() -> Grounding {
    let names = [
        "Back",
        "Forward",
        "Reload",
        "Home",
        "Address and search bar",
        "Bookmark this tab",
        "Extensions",
        "Downloads",
        "Open application menu",
        "New Tab",
        "Close Tab",
        "Wikipedia, the free encyclopedia",
        "Search Wikipedia",
        "Search",
        "Main page",
        "Contents",
        "Current events",
        "Random article",
        "About Wikipedia",
        "Contact us",
        "Help",
        "Learn to edit",
        "Community portal",
        "Recent changes",
        "Upload file",
        "Create account",
        "Log in",
        "Talk",
        "Read",
        "View source",
        "View history",
        "Tools",
        "",
        "",
        "Language",
        "Download as PDF",
        "Printable version",
        "Permanent link",
        "Page information",
        "Cite this page",
    ];
    Grounding::Marks(
        names
            .iter()
            .enumerate()
            .map(|(i, n)| Element {
                mark: i as u32 + 1,
                role: if i % 3 == 0 { Role::Link } else { Role::Button },
                name: n.to_string(),
                bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
                enabled: i != 7,
                secure: false,
                focused: false,
                value: None,
            })
            .collect(),
    )
}

#[test]
fn compact_turn_is_smaller_than_the_old_format() {
    let g = typical_screen();
    let hist: Vec<HistoryItem> = [
        "open_url \"https://wikipedia.org\"",
        "click #13",
        "type #13 \"Rust\"",
    ]
    .iter()
    .map(|s| HistoryItem::note(*s))
    .collect();
    let task = "Find the Wikipedia article about the Rust programming language";
    let new = user_turn(task, &g, &hist, None, false);
    // The pre-compaction layout, rebuilt for comparison.
    let mut old = format!("Elements on screen:\n{}\n\nYour last steps:\n", g.as_text());
    for h in &hist {
        old.push_str(&format!("- {}\n", h.summary));
    }
    old.push_str(&format!(
        "\nTask: {task}\n\nRespond with the next action as JSON."
    ));
    eprintln!(
        "MEASURED user turn, 40 elements: old {} chars (~{} tokens), new {} chars (~{} tokens)",
        old.len(),
        old.len() / 4,
        new.len(),
        new.len() / 4
    );
    assert!(new.len() * 10 < old.len() * 9, "at least 10% smaller");
}
