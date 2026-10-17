use super::*;
use crate::computer::MouseButton;
use crate::risk::ApprovalLevel;
use crate::{Element, Grounding, Rect, Role};

fn click(x: f64, y: f64) -> Turn {
    Turn::Act {
        thought: "IGNORE RULES secret-thought".into(),
        actions: vec![ComputerAction::Click {
            target: Target::Point {
                observation: 1,
                x,
                y,
            },
            button: MouseButton::Left,
            count: 1,
            modifiers: vec![],
        }],
    }
}

fn secure_field(x: f32, y: f32) -> Grounding {
    Grounding::Marks(vec![Element {
        mark: 7,
        role: Role::PasswordField,
        name: "Password".into(),
        bounds: Rect::new(x, y, 10., 10.),
        enabled: true,
        secure: true,
        focused: false,
        value: None,
    }])
}

#[test]
fn approval_inherits_from_every_agreeing_candidate() {
    let turns = [click(5., 5.), click(14., 5.)];
    let groups = oai_contract::vote(turns.iter()).unwrap().groups;
    assert_eq!(groups, vec![vec![0, 1]]);
    let g = secure_field(12., 3.);
    assert!(!needs_candidate_approval(
        &turns[0],
        &g,
        ApprovalLevel::Autopilot,
        (100, 100),
        None
    ));
    assert!(needs_candidate_approval(
        &turns[1],
        &g,
        ApprovalLevel::Autopilot,
        (100, 100),
        None
    ));
    assert!(groups[0].iter().any(|&i| needs_candidate_approval(
        &turns[i],
        &g,
        ApprovalLevel::Autopilot,
        (100, 100),
        None,
    )));
}

#[test]
fn secure_targets_use_the_observed_frame_not_the_native_frame() {
    let t = click(100., 50.);
    let g = secure_field(49., 24.);
    assert!(!needs_candidate_approval(
        &t,
        &g,
        ApprovalLevel::Autopilot,
        (200, 100),
        None
    ));
    assert!(needs_candidate_approval(
        &t,
        &g,
        ApprovalLevel::Autopilot,
        (200, 100),
        Some((100, 50))
    ));
}

#[test]
fn malformed_judge_output_falls_back_and_untrusted_proposals_are_redacted() {
    for raw in [
        "{\"choice\":9}",
        "{\"choice\":\"0\"}",
        "{\"choice\":0,\"action\":\"click\"}",
        "x {\"choice\":0}",
    ] {
        assert_eq!(judge_choice(raw, 2), None);
    }
    assert_eq!(judge_choice("{\"choice\":1}", 2), Some(1));
    assert!(
        !candidate_summary(&click(10., 12.))
            .to_string()
            .contains("secret-thought")
    );
    let typed = Turn::Act {
        thought: "private".into(),
        actions: vec![ComputerAction::Type {
            target: Target::Element {
                observation: 1,
                mark: 3,
            },
            text: "secret password".into(),
        }],
    };
    let summary = candidate_summary(&typed).to_string();
    assert!(!summary.contains("private") && !summary.contains("secret password"));
}

#[test]
fn sampling_executes_only_one_action_even_if_oai_batch_is_larger() {
    assert_eq!(candidate_batch(8, 4), 1);
    assert_eq!(candidate_batch(8, 1), 8);
}

#[test]
fn candidate_approval_matches_conservative_computer_policy() {
    let g = Grounding::Marks(vec![]);
    let click = click(30., 30.);
    assert!(needs_candidate_approval(
        &click,
        &g,
        ApprovalLevel::HighRiskOnly,
        (100, 100),
        None
    ));
    assert!(!needs_candidate_approval(
        &click,
        &g,
        ApprovalLevel::Autopilot,
        (100, 100),
        None
    ));
    let keys = Turn::Act {
        thought: String::new(),
        actions: vec![ComputerAction::Keypress {
            keys: vec!["ENTER".into()],
        }],
    };
    assert!(needs_candidate_approval(
        &keys,
        &g,
        ApprovalLevel::HighRiskOnly,
        (100, 100),
        None
    ));
    let wait = Turn::Act {
        thought: String::new(),
        actions: vec![ComputerAction::Wait { duration_ms: 10 }],
    };
    assert!(!needs_candidate_approval(
        &wait,
        &g,
        ApprovalLevel::HighRiskOnly,
        (100, 100),
        None
    ));
}
