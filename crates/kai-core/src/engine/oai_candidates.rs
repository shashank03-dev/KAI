//! Candidate summaries, judge parsing and inherited risk checks for the OpenAI contract.
use crate::Grounding;
use crate::Risk;
use crate::computer::{ComputerAction, Target};
use crate::oai_contract::Turn;
use crate::reply::StepReply;
use crate::risk::{self, ApprovalLevel};
use crate::schema::Action;
use serde_json::json;

/// Give the tie judge only action types and coordinates. Never forward model
/// thoughts, typed content, URLs or app names from untrusted candidates.
pub(super) fn candidate_summary(turn: &Turn) -> serde_json::Value {
    match turn {
        Turn::Act { actions, .. } => actions.first().map_or(json!({"kind":"none"}), |action| {
            let targets = action
                .targets()
                .iter()
                .map(|t| match t {
                    Target::Element { mark, .. } => json!({"mark":mark}),
                    Target::Point { x, y, .. } => json!({"x":x,"y":y}),
                })
                .collect::<Vec<_>>();
            json!({"kind":format!("{:?}", action.kind()), "targets": targets})
        }),
        Turn::Done { .. } => json!({"kind":"done"}),
        Turn::Ask { .. } => json!({"kind":"ask"}),
    }
}

pub(super) fn judge_choice(raw: &str, options: usize) -> Option<usize> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    usize::try_from(object.get("choice")?.as_u64()?)
        .ok()
        .filter(|&i| i < options)
}

/// A vote is a vote on one action. Later actions cannot bypass the cluster
/// check or the fact that only one candidate is selected for execution.
pub(super) fn candidate_batch(batch: usize, candidates: usize) -> usize {
    if candidates > 1 { 1 } else { batch }
}

/// Evaluate candidate actions against the observed screen. The model may see
/// a larger native frame; its pixel positions must be scaled before looking
/// for secure elements in the observed accessibility tree.
pub(super) fn needs_candidate_approval(
    turn: &Turn,
    g: &Grounding,
    level: ApprovalLevel,
    size: (u32, u32),
    seen: Option<(u32, u32)>,
) -> bool {
    let Turn::Act { actions, .. } = turn else {
        return false;
    };
    let Some(original) = actions.first() else {
        return false;
    };
    let mut action = original.clone();
    if let Some(to) = seen
        && size != to
        && size.0 > 0
        && size.1 > 0
    {
        for t in action.targets_mut() {
            if let Target::Point { x, y, .. } = t {
                *x = (*x * to.0 as f64 / size.0 as f64).min(to.0 as f64 - 1.);
                *y = (*y * to.1 as f64 / size.1 as f64).min(to.1 as f64 - 1.);
            }
        }
    }
    let raw = json!({"thought":"candidate", "action":"computer", "computer":action, "risk":"low"});
    let Ok(step) = StepReply::parse_computer(&raw.to_string(), None) else {
        return true;
    };
    let mut d = step.decision;
    d.risk = Risk::High;
    d.target = None;
    d.text = None;
    d.action = match &action {
        ComputerAction::Type { text, .. } => {
            d.text = Some(text.clone());
            Action::Type
        }
        ComputerAction::Observe | ComputerAction::Wait { .. } | ComputerAction::Move { .. } => {
            d.risk = Risk::None;
            Action::Wait
        }
        ComputerAction::Scroll { .. } => {
            d.risk = Risk::Low;
            Action::Scroll
        }
        ComputerAction::Click { .. } | ComputerAction::Drag { .. } => Action::Click,
        ComputerAction::Keypress { keys } => {
            d.keys = keys.clone();
            Action::Key
        }
        ComputerAction::OpenApp { name } => {
            d.text = Some(name.clone());
            Action::OpenApp
        }
        ComputerAction::OpenUrl { url } => {
            d.text = Some(url.clone());
            Action::OpenUrl
        }
    };
    let mut risk_level = risk::classify(&d, g);
    let mut mandatory = risk::is_payment_or_password(&d, g);
    for target in action.targets() {
        let marks = match target {
            Target::Element { mark, .. } => vec![*mark],
            Target::Point { x, y, .. } => match g {
                Grounding::Marks(elements) => elements
                    .iter()
                    .filter(|el| {
                        let b = el.bounds;
                        *x >= b.x as f64
                            && *y >= b.y as f64
                            && *x < (b.x + b.w) as f64
                            && *y < (b.y + b.h) as f64
                    })
                    .map(|el| el.mark)
                    .collect(),
                _ => Vec::new(),
            },
        };
        for mark in marks {
            let mut probe = d.clone();
            probe.target = Some(mark);
            risk_level = risk_level.max(risk::classify(&probe, g));
            mandatory |= risk::is_payment_or_password(&probe, g);
        }
    }
    risk::needs_approval(level, risk_level, mandatory)
}
