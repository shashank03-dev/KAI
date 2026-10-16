//! Tests for Home's starter cards (`home.rs`).

use super::*;

fn skill(name: &str, description: &str, builtin: bool) -> kai_core::Skill {
    let md = format!("---\nname: {name}\ndescription: {description}\n---\n# {name}\n");
    let mut s = kai_core::skill::load_from_str(&md, std::path::Path::new(name)).unwrap();
    s.builtin = builtin;
    s
}

#[test]
fn your_own_skills_come_first_as_tasks_to_edit() {
    let skills = [
        skill("run-terminal-task", "Runs a terminal task.", true),
        skill(
            "tidy-downloads",
            "Move old files out of Downloads. Use when the user asks to tidy downloads.",
            false,
        ),
    ];
    let got = starters(&skills, &BTreeSet::new());
    assert_eq!(
        got[0],
        Starter::Own {
            skill: "tidy-downloads".into(),
            task: "Move old files out of Downloads.".into(),
        }
    );
    assert!(matches!(
        got[1],
        Starter::Example {
            skill: "run-terminal-task",
            ..
        }
    ));
}

#[test]
fn switched_off_or_missing_skills_give_no_cards() {
    let skills = [skill("run-terminal-task", "Runs a terminal task.", true)];
    let off: BTreeSet<String> = ["run-terminal-task".to_string()].into();
    assert!(starters(&skills, &off).is_empty());
    assert!(starters(&[], &BTreeSet::new()).is_empty());
    // A skill of yours named like a built-in is still yours, not the example.
    let mine = [skill("run-terminal-task", "Mine.", false)];
    assert!(matches!(
        starters(&mine, &BTreeSet::new())[0],
        Starter::Own { .. }
    ));
}

#[test]
fn starters_are_capped() {
    let skills: Vec<_> = (0..10)
        .map(|i| skill(&format!("mine-{i}"), "A task.", false))
        .collect();
    assert_eq!(starters(&skills, &BTreeSet::new()).len(), MAX_STARTERS);
}
