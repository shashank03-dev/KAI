//! Tests for the Skills page helpers (`skills.rs`).

use super::*;

#[test]
fn drafted_skills_pass_the_validator() {
    let mut trace = kai_core::Trace::new("Open bmsce.ac.in and find the IEEE CS page");
    let d = kai_core::Decision::parse_checked(
        r#"{"thought":"go","action":"open_url","text":"https://bmsce.ac.in","risk":"none"}"#,
    )
    .unwrap();
    trace.push(kai_core::trace::StepRecord::new(
        0,
        &d,
        Default::default(),
        "x",
    ));
    let md = draft_from(&trace.task.clone(), Some(&trace));
    let name = slug(&trace.task);
    let s = kai_core::skill::load_from_str(&md, Path::new(&name)).unwrap();
    assert_eq!(s.steps.len(), 1);
    assert_eq!(s.front.name, "open-bmsce-ac-in-and-find");
}

#[test]
fn frontmatter_is_left_out_of_the_body() {
    let md = "---\nname: x\ndescription: y\n---\n# Title\n\nBody\n";
    assert_eq!(body_of(md), "# Title\n\nBody\n");
    assert_eq!(body_of("# No frontmatter"), "# No frontmatter");
    assert_eq!(body_of("---\nunclosed"), "---\nunclosed");
}

#[test]
fn slugs_follow_the_name_rule() {
    assert_eq!(slug("Play lofi -- hip hop!"), "play-lofi-hip-hop");
    assert_eq!(slug("!!!"), "my-skill");
    assert!(kai_core::skill::valid_name(&slug(&"word ".repeat(40))));
}
