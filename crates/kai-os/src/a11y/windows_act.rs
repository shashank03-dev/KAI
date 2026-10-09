//! Windows: perform a planned action through UI Automation patterns, no
//! pointer move. Press → Invoke, Toggle, ExpandCollapse or SelectionItem,
//! chosen by control role. Fill → Value.SetValue on an empty, writable,
//! non-password field, then read back to confirm. Runs on a blocking thread.

use super::live::Plan;
use super::windows::UiaRef;
use kai_core::Role;
use uiautomation::UIAutomation;
use uiautomation::patterns::{
    UIExpandCollapsePattern, UIInvokePattern, UISelectionItemPattern, UITogglePattern,
    UIValuePattern,
};
use uiautomation::types::ExpandCollapseState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pattern {
    Invoke,
    Toggle,
    ExpandCollapse,
    SelectionItem,
}

/// Patterns to try for a plain click, best first.
fn press_patterns(role: Role) -> &'static [Pattern] {
    use Pattern::*;
    match role {
        Role::Button | Role::Link => &[Invoke, Toggle, ExpandCollapse],
        Role::MenuItem => &[Invoke, ExpandCollapse],
        Role::Checkbox => &[Toggle, Invoke],
        Role::Radio | Role::Tab => &[SelectionItem, Invoke],
        Role::ComboBox => &[ExpandCollapse],
        _ => &[],
    }
}

pub fn act(node: &UiaRef, plan: &Plan, role: Role) -> anyhow::Result<bool> {
    // Joins this blocking thread to the MTA (see `UiaRef`).
    let _auto = UIAutomation::new()?;
    let el = &node.0;
    // Live checks: the element may be gone or disabled since the observation.
    let Ok(enabled) = el.is_enabled() else {
        return Ok(false);
    };
    if !enabled || el.is_password().unwrap_or(true) {
        return Ok(false);
    }
    match plan {
        Plan::Press => press(node, role),
        Plan::Fill(text) => fill(node, text),
    }
}

fn press(node: &UiaRef, role: Role) -> anyhow::Result<bool> {
    let el = &node.0;
    for p in press_patterns(role) {
        // None: pattern not supported, try the next one.
        let done: Option<Result<(), String>> = match p {
            Pattern::Invoke => el
                .get_pattern::<UIInvokePattern>()
                .ok()
                .map(|x| x.invoke().map_err(|e| e.to_string())),
            Pattern::Toggle => el
                .get_pattern::<UITogglePattern>()
                .ok()
                .map(|x| x.toggle().map_err(|e| e.to_string())),
            Pattern::SelectionItem => el
                .get_pattern::<UISelectionItemPattern>()
                .ok()
                .map(|x| x.select().map_err(|e| e.to_string())),
            Pattern::ExpandCollapse => {
                el.get_pattern::<UIExpandCollapsePattern>()
                    .ok()
                    .and_then(|x| match x.get_state() {
                        Ok(ExpandCollapseState::LeafNode) => None,
                        Ok(ExpandCollapseState::Expanded) => {
                            Some(x.collapse().map_err(|e| e.to_string()))
                        }
                        _ => Some(x.expand().map_err(|e| e.to_string())),
                    })
            }
        };
        match done {
            None => continue,
            Some(Ok(())) => return Ok(true),
            Some(Err(e)) => {
                log::debug!("UIA {p:?} failed: {e}");
                return Ok(false);
            }
        }
    }
    Ok(false)
}

fn fill(node: &UiaRef, text: &str) -> anyhow::Result<bool> {
    let el = &node.0;
    let Ok(value) = el.get_pattern::<UIValuePattern>() else {
        return Ok(false);
    };
    if value.is_readonly().unwrap_or(true) {
        return Ok(false);
    }
    // Only an empty field: see `Plan::Fill` for why.
    match value.get_value() {
        Ok(v) if v.is_empty() => {}
        _ => return Ok(false),
    }
    let _ = el.set_focus();
    if let Err(e) = value.set_value(text) {
        log::debug!("UIA SetValue failed: {e}");
        return Ok(false);
    }
    let now = value.get_value().unwrap_or_default();
    if now == text {
        return Ok(true);
    }
    if now.is_empty() {
        return Ok(false);
    }
    let _ = value.set_value("");
    if value.get_value().map(|v| v.is_empty()).unwrap_or(false) {
        Ok(false)
    } else {
        anyhow::bail!("UI Automation text entry left {now:?} instead of {text:?}")
    }
}
