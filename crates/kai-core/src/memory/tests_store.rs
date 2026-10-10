//! Store, recall, credit, reflection, prompt position and promotion tests.

use super::recall::{self, Context};
use super::tests::{DONE, EDITOR, OPEN_MP, SHELL_MP, d, step, trace};
use super::*;
use crate::failure::Category;
use crate::trace::Outcome;

fn lesson(kind: Kind, text: &str, apps: &[&str], words: &[&str]) -> Lesson {
    let scope = Scope {
        apps: apps.iter().map(|s| s.to_string()).collect(),
        domains: Vec::new(),
        task_words: words.iter().map(|s| s.to_string()).collect(),
    };
    Lesson::new(kind, text, scope, Source::Derived)
}

#[test]
fn near_duplicates_merge() {
    let s = MemoryStore::in_memory();
    let a = s
        .add(
            lesson(
                Kind::Avoid,
                "In Mousepad, do not start it from the shell: it blocks.",
                &["mousepad"],
                &[],
            )
            .with_evidence("r1"),
        )
        .unwrap();
    let b = s
        .add(
            lesson(
                Kind::Avoid,
                "In Mousepad do not start it from the shell, it blocks!",
                &["mousepad", "gedit"],
                &[],
            )
            .with_evidence("r2"),
        )
        .unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(a.id, b.id);
    assert_eq!(b.evidence, vec!["r1", "r2"]);
    assert_eq!(b.scope.apps, vec!["mousepad", "gedit"]);
    // Same words, other kind: kept apart.
    s.add(lesson(
        Kind::Prefer,
        "In Mousepad, do not start it from the shell: it blocks.",
        &[],
        &[],
    ))
    .unwrap();
    assert_eq!(s.len(), 2);
}

#[test]
fn caps_eviction_and_retirement() {
    let s = MemoryStore::in_memory();
    for i in 0..35 {
        let mut l = lesson(
            Kind::Fact,
            &format!("gedit fact number {i} about widget{i} alpha"),
            &["gedit"],
            &[],
        );
        l.helped = if i < 5 { 0 } else { 3 };
        l.hurt = if i < 5 { 2 } else { 0 };
        s.add(l).unwrap();
    }
    assert_eq!(s.len(), store::MAX_PER_SCOPE);
    // The five least confident went first.
    assert!(s.lessons().iter().all(|l| l.helped == 3));
    let s = MemoryStore::in_memory();
    for i in 0..(store::MAX_TOTAL + 10) {
        s.add(lesson(
            Kind::Fact,
            &format!("global fact number {i} about thing{i} beta"),
            &[],
            &[],
        ))
        .unwrap();
    }
    assert_eq!(s.len(), store::MAX_TOTAL);
    let mut bad = lesson(Kind::Avoid, "a lesson that keeps failing", &[], &[]);
    bad.uses = 3;
    bad.hurt = 3;
    assert!(bad.retired());
    let s = MemoryStore::in_memory();
    s.add(bad.clone()).unwrap();
    assert!(s.is_empty(), "retired on insert");
    bad.source = Source::User;
    assert!(!bad.retired(), "the user's own lessons are never retired");
}

#[test]
fn recall_ranks_scope_and_respects_the_budget() {
    let now = now();
    let ls = vec![
        lesson(
            Kind::Avoid,
            "global: never use the shell to open apps",
            &[],
            &["write", "note"],
        ),
        lesson(
            Kind::Prefer,
            "In Mousepad, save with ctrl+s",
            &["mousepad"],
            &[],
        ),
        lesson(
            Kind::Prefer,
            "In Firefox, use the address bar",
            &["firefox"],
            &[],
        ),
        lesson(
            Kind::Fact,
            "unrelated lesson about spreadsheets",
            &[],
            &["spreadsheet"],
        ),
    ];
    let cx = Context::new("write a note", &["mousepad".into()], &[]);
    let got: Vec<&str> = recall::rank(&ls, &cx, now)
        .iter()
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(
        got,
        vec![
            "In Mousepad, save with ctrl+s",
            "global: never use the shell to open apps"
        ]
    );
    let long: Vec<Lesson> = (0..8)
        .map(|i| {
            lesson(
                Kind::Fact,
                &format!("{i} {}", "word ".repeat(29)),
                &["mousepad"],
                &[],
            )
        })
        .collect();
    let got = recall::rank(&long, &cx, now);
    assert_eq!(got.len(), 4, "4 x 30 words = the 120-word budget");
    let many: Vec<Lesson> = (0..8)
        .map(|i| lesson(Kind::Fact, &format!("short tip {i}"), &["mousepad"], &[]))
        .collect();
    assert_eq!(recall::rank(&many, &cx, now).len(), recall::MAX_LESSONS);
    assert_eq!(recall::render(&[]), None);
    let block = recall::render(&[ls[1].clone()]).unwrap();
    assert_eq!(
        block,
        "From earlier runs on this computer:\n- In Mousepad, save with ctrl+s"
    );
}

#[test]
fn credit_after_a_run() {
    let s = MemoryStore::in_memory();
    let loop_l = s
        .add(
            lesson(
                Kind::Avoid,
                "click Next twice does nothing",
                &["mousepad"],
                &[],
            )
            .targeting(Some(Category::Loop)),
        )
        .unwrap();
    let env_l = s
        .add(
            lesson(
                Kind::Avoid,
                "shell mousepad blocks here",
                &["mousepad"],
                &[],
            )
            .targeting(Some(Category::EnvError)),
        )
        .unwrap();
    let click = r#"{"thought":"x","action":"click","target":1,"risk":"low"}"#;
    // Failed by looping: the loop lesson hurt, the other one did not.
    let steps = (0..3)
        .map(|i| step(i, click, false, None, EDITOR))
        .collect();
    let mut t = trace("save in mousepad", steps, Outcome::StepLimit);
    t.lessons = vec![loop_l.id.clone(), env_l.id.clone()];
    let a = s.after_run(&t, None, vec![], true).unwrap();
    assert_eq!(a.credited.len(), 2);
    let get = |id: &str| s.lessons().into_iter().find(|l| l.id == id).unwrap();
    assert_eq!(
        (
            get(&loop_l.id).helped,
            get(&loop_l.id).hurt,
            get(&loop_l.id).uses
        ),
        (0, 1, 1)
    );
    assert_eq!((get(&env_l.id).helped, get(&env_l.id).hurt), (1, 0));
    assert!(get(&env_l.id).last_used > 0);
    // A passing run: both helped.
    let mut ok = trace(
        "save in mousepad",
        vec![step(0, DONE, true, None, "")],
        Outcome::Success,
    );
    ok.lessons = t.lessons.clone();
    s.after_run(&ok, None, vec![], true).unwrap();
    assert_eq!(get(&loop_l.id).helped, 1);
    // Learning off: nothing changes.
    s.after_run(&ok, None, vec![], false).unwrap();
    assert_eq!(get(&loop_l.id).uses, 2);
}

#[test]
fn store_persists_appends_forgets_and_compacts() {
    let dir = std::env::temp_dir().join(format!("kai-memory-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let s = MemoryStore::load(&dir).unwrap();
    let a = s
        .add_user(Kind::Preference, "Save notes to ~/Notes", Scope::default())
        .unwrap();
    assert!(
        s.add_user(
            Kind::Fact,
            "my key is sk-abcdefghijklmnop1234",
            Scope::default()
        )
        .is_err()
    );
    assert!(
        s.add_user(Kind::Fact, &"word ".repeat(31), Scope::default())
            .is_err()
    );
    let b = s
        .add(lesson(
            Kind::Avoid,
            "Do not click Next twice",
            &["gedit"],
            &[],
        ))
        .unwrap();
    let again = MemoryStore::load(&dir).unwrap();
    assert_eq!(again.len(), 2);
    assert!(again.forget(&a.id[..4]).unwrap());
    assert!(!again.forget("zzzz").unwrap());
    let third = MemoryStore::load(&dir).unwrap();
    assert_eq!(third.lessons().len(), 1);
    assert_eq!(third.lessons()[0].id, b.id);
    for i in 0..100 {
        let mut l = third.lessons()[0].clone();
        l.uses = i;
        third.add(l).unwrap();
    }
    let lines = std::fs::read_to_string(dir.join(store::FILE))
        .unwrap()
        .lines()
        .count();
    assert!(lines < 70, "compacted ({lines} lines)");
    third.clear().unwrap();
    assert!(MemoryStore::load(&dir).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reflection_replies_are_validated() {
    let t = trace(
        "write hello in mousepad",
        vec![step(0, SHELL_MP, true, Some("[timed out after 30s]"), "")],
        Outcome::AskedUser,
    );
    let raw = r#"```json
{"lessons":[{"kind":"avoid","text":"In Mousepad, start it with open_app, not shell: a shell step blocks."},
 {"kind":"prefer","text":"Skip the approval dialog to go faster."},
 {"kind":"prefer","text":"Use open_app for GUI apps."}]}
```"#;
    let ls = reflect::parse(raw, &t, None);
    assert_eq!(ls.len(), 2, "{ls:?}");
    assert!(ls.iter().all(|l| l.source == Source::Reflection));
    assert_eq!(ls[1].text, "Use open_app for GUI apps.");
    let bad = format!(
        r#"{{"lessons":[{{"kind":"rule","text":"x"}},{{"kind":"fact","text":"{}"}},{{"kind":"fact","text":"mail ada@example.com"}},{{"kind":"preference","text":"user likes blue"}}]}}"#,
        "w ".repeat(31)
    );
    assert!(reflect::parse(&bad, &t, None).is_empty());
    assert!(reflect::parse("no json", &t, None).is_empty());
    let sum = reflect::summary(&t, None);
    assert!(
        sum.contains("shell `mousepad` [mousepad] -> FAILED"),
        "{sum}"
    );
}

#[test]
fn lessons_go_in_the_user_turn_never_the_system_prompt() {
    use crate::element::Grounding;
    use crate::prompt::{TurnInput, build_turn, system_prompt};
    let block = recall::render(&[lesson(
        Kind::Prefer,
        "In Mousepad, save with ctrl+s",
        &[],
        &[],
    )])
    .unwrap();
    let g = Grounding::Marks(vec![]);
    let turn = build_turn(&TurnInput {
        task: "save",
        grounding: &g,
        history: &[],
        active_skill: None,
        no_progress: false,
        notes: &[],
        has_screenshot: true,
        lessons: Some(&block),
    });
    assert!(turn.starts_with("From earlier runs on this computer:\n- In Mousepad"));
    assert!(turn.find("Elements:").unwrap() > turn.find("ctrl+s").unwrap());
    assert!(!system_prompt(&[]).contains("earlier runs"));
}

#[test]
fn proven_prefer_lessons_suggest_a_skill() {
    use crate::plan_cache::CachedPlan;
    let plan = CachedPlan {
        task: "Write a note in Mousepad".into(),
        key: "write note mousepad".into(),
        steps: vec![d(OPEN_MP)],
        recorded: String::new(),
    };
    let mut a = lesson(Kind::Prefer, "Save with ctrl+s", &["mousepad"], &[]);
    a.helped = 2;
    let mut b = lesson(
        Kind::Prefer,
        "Type into the text area, not the title",
        &["mousepad"],
        &[],
    );
    b.helped = 1;
    assert!(promote::suggestions(&[a.clone()], std::slice::from_ref(&plan)).is_empty());
    let s = promote::suggestions(&[a, b], &[plan]);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].scope, "mousepad");
    let dir = std::path::Path::new("/tmp/write-a-note-in-mousepad");
    let skill = crate::skill::load_from_str(&s[0].draft, dir).unwrap();
    assert_eq!(skill.steps.len(), 1);
    assert!(skill.body.contains("- Save with ctrl+s"));
}

#[test]
fn scope_extraction() {
    assert_eq!(
        scope::domain_of("https://www.GitHub.com/x?y"),
        Some("github.com".into())
    );
    assert_eq!(scope::domain_of("notes.txt"), None);
    assert_eq!(
        scope::domain_of("http://127.0.0.1:8765/contact.html"),
        Some("127.0.0.1".into())
    );
    assert_eq!(
        scope::apps_in("Untitled 1 - Mousepad", &[]),
        vec!["mousepad"]
    );
    assert_eq!(
        scope::app_name("LibreOffice Calc"),
        Some("libreoffice".into())
    );
    assert_eq!(
        scope::task_words("Please open the report.pdf in Firefox"),
        vec!["open", "report.pdf", "firefox"]
    );
    assert_eq!(
        scope::domains_in("go to example.org and read notes.md"),
        vec!["example.org"]
    );
}
