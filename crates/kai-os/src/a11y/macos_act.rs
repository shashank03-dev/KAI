//! macOS: perform a planned action through the Accessibility API, no pointer
//! move. Press → the element's `AXPress` action. Fill → set `AXValue` on an
//! empty, settable, non-secure field, then read back to confirm. Runs on a
//! blocking thread.

use super::live::Plan;
use super::macos::{AxRef, attr, boolean, string};
use super::macos_roles::map_role;
use kai_core::Role;
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFBoolean, CFString};
use std::ptr::NonNull;

pub fn act(node: &AxRef, plan: &Plan, role: Role) -> anyhow::Result<bool> {
    let el = &*node.0;
    // Live checks: the element may be gone (no role) or disabled since the
    // observation, and a secure field is never filled or pressed this way.
    // Text views report no `AXEnabled` at all (TextEdit's, seen on macOS
    // 15): missing means enabled, as in the tree read.
    let Some(role_now) = attr(el, "AXRole").and_then(|v| string(&v)) else {
        return Ok(false);
    };
    let enabled = attr(el, "AXEnabled")
        .and_then(|v| boolean(&v))
        .unwrap_or(true);
    let subrole = attr(el, "AXSubrole")
        .and_then(|v| string(&v))
        .unwrap_or_default();
    if !enabled || map_role(&role_now, &subrole) != role || role == Role::PasswordField {
        return Ok(false);
    }
    match plan {
        Plan::Press => Ok(press(el)),
        Plan::Fill(text) => fill(el, text),
    }
}

fn press(el: &AXUIElement) -> bool {
    // SAFETY: a plain action call on a live element.
    let err = unsafe { el.perform_action(&CFString::from_static_str("AXPress")) };
    if err != AXError::Success {
        log::debug!("AXPress failed: {err:?}");
    }
    err == AXError::Success
}

fn fill(el: &AXUIElement, text: &str) -> anyhow::Result<bool> {
    let value = CFString::from_static_str("AXValue");
    let mut settable = 0u8;
    // SAFETY: `settable` is a valid out pointer for a Boolean.
    let err = unsafe { el.is_attribute_settable(&value, NonNull::from(&mut settable)) };
    if err != AXError::Success || settable == 0 {
        return Ok(false);
    }
    // Only an empty field: see `Plan::Fill` for why.
    if current(el).is_none_or(|v| !v.is_empty()) {
        return Ok(false);
    }
    // SAFETY: plain setters on a live element with CF values.
    unsafe {
        el.set_attribute_value(
            &CFString::from_static_str("AXFocused"),
            CFBoolean::new(true),
        );
        if el.set_attribute_value(&value, &CFString::from_str(text)) != AXError::Success {
            return Ok(false);
        }
    }
    let now = current(el).unwrap_or_default();
    if now == text {
        return Ok(true);
    }
    if now.is_empty() {
        return Ok(false);
    }
    // SAFETY: as above.
    unsafe { el.set_attribute_value(&value, &CFString::from_static_str("")) };
    if current(el).is_some_and(|v| v.is_empty()) {
        Ok(false)
    } else {
        anyhow::bail!("accessibility text entry left {now:?} instead of {text:?}")
    }
}

fn current(el: &AXUIElement) -> Option<String> {
    attr(el, "AXValue").and_then(|v| string(&v))
}
