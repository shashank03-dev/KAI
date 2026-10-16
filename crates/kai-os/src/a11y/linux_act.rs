//! Linux: perform a planned action through AT-SPI with no pointer move.
//! Press → the element's Action interface, picked by action *name* (never by
//! index, so a list item's "activate" or an entry's "activate" = Enter is
//! never fired by accident). Fill → EditableText on an empty field, then read
//! back to confirm.

use super::NodeRef;
use super::linux::{conn, extents};
use super::live::{Plan, same_place};
use atspi::proxy::accessible::ObjectRefExt;
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::{Role as A, State};
use kai_core::{Element, Rect, Role};
use std::time::Duration;

/// Budget for the checks before acting; past it the mouse takes over.
const CHECK_BUDGET: Duration = Duration::from_millis(800);
/// Budget for the action call itself. Past it the outcome is unknown, which
/// is an error (not a fallback) so the step is not done twice.
const ACT_BUDGET: Duration = Duration::from_millis(1500);
/// Re-reads (40 ms apart) of a field that still looks empty after an edit.
const SETTLE_POLLS: usize = 3;
/// Toolkits that report success for EditableText edits on their fields but
/// do not apply them (measured: Firefox 140 ESR on web page inputs).
const NO_FILL_TOOLKITS: &[&str] = &["Gecko"];

/// Action names to try for a plain click, best first.
pub(super) fn press_names(role: Role) -> &'static [&'static str] {
    match role {
        // Gecko names the checkbox action by what it will do: check/uncheck.
        Role::Checkbox => &["toggle", "click", "press", "check", "uncheck"],
        Role::Radio => &["toggle", "click", "press", "select"],
        Role::ComboBox => &["press", "click", "open"],
        Role::Link => &["jump", "click", "press", "activate"],
        Role::Tab => &["switch", "click", "press", "activate"],
        Role::Button | Role::MenuItem => &["click", "press", "activate", "toggle"],
        _ => &[],
    }
}

/// Index of the first preferred action the element offers.
pub(super) fn pick_action(offered: &[String], wanted: &[&str]) -> Option<usize> {
    wanted.iter().find_map(|w| {
        offered
            .iter()
            .position(|o| o.trim().eq_ignore_ascii_case(w))
    })
}

pub async fn act(
    node: &NodeRef,
    plan: &Plan,
    seen: &Element,
    ctx: &kai_core::computer::ExecutionContext,
) -> anyhow::Result<bool> {
    match plan {
        Plan::Press => press(node, seen, ctx).await,
        Plan::Fill(text) => fill(node, text, seen.bounds, ctx).await,
    }
}

async fn press(
    node: &NodeRef,
    seen: &Element,
    ctx: &kai_core::computer::ExecutionContext,
) -> anyhow::Result<bool> {
    let role = seen.role;
    let bus = conn().await?.connection();
    let prep = async {
        let p = node.as_accessible_proxy(bus).await.ok()?;
        if !usable(&p, seen.bounds).await? {
            return None;
        }
        let action = p.proxies().await.ok()?.action().await.ok()?;
        let n = action.n_actions().await.ok()?;
        let mut names = Vec::with_capacity(n.max(0) as usize);
        for i in 0..n {
            names.push(action.get_name(i).await.unwrap_or_default());
        }
        let Some(idx) = pick_action(&names, press_names(role)) else {
            log::debug!("a11y press: no fitting action for {role:?} in {names:?}");
            return None;
        };
        Some((action, idx as i32, names))
    };
    let Ok(Some((action, idx, names))) = tokio::time::timeout(CHECK_BUDGET, prep).await else {
        return Ok(false);
    };
    log::debug!("a11y press: action {idx} of {names:?}");
    ctx.check()?;
    match tokio::time::timeout(ACT_BUDGET, action.do_action(idx)).await {
        Ok(Ok(done)) => Ok(done),
        Ok(Err(e)) => {
            // The call failed before the app ran it (gone object, bad index).
            log::debug!("a11y do_action failed: {e}");
            Ok(false)
        }
        Err(_) => anyhow::bail!("accessibility action timed out; its effect is unknown"),
    }
}

async fn fill(
    node: &NodeRef,
    text: &str,
    seen: Rect,
    ctx: &kai_core::computer::ExecutionContext,
) -> anyhow::Result<bool> {
    let bus = conn().await?.connection();
    let prep = async {
        let p = node.as_accessible_proxy(bus).await.ok()?;
        if !usable(&p, seen).await? {
            return None;
        }
        let state = p.get_state().await.ok()?;
        let role = p.get_role().await.ok()?;
        if !state.contains(State::Editable) || state.contains(State::ReadOnly) {
            log::debug!("a11y fill: not editable ({state:?})");
            return None;
        }
        if matches!(role, A::PasswordText) {
            return None;
        }
        let toolkit = toolkit(&p, bus).await.unwrap_or_default();
        if NO_FILL_TOOLKITS
            .iter()
            .any(|t| toolkit.eq_ignore_ascii_case(t))
        {
            log::debug!("a11y fill: {toolkit} ignores EditableText; using the keyboard");
            return None;
        }
        let px = p.proxies().await.ok()?;
        let Ok(edit) = px.editable_text().await else {
            log::debug!("a11y fill: no EditableText interface");
            return None;
        };
        let read = px.text().await.ok()?;
        // Only an empty field: see `Plan::Fill` for why.
        let count = read.character_count().await.ok()?;
        if count != 0 {
            log::debug!("a11y fill: field holds {count} characters");
            return None;
        }
        let comp = px.component().await.ok();
        Some((edit, read, comp))
    };
    let Ok(Some((edit, read, comp))) = tokio::time::timeout(CHECK_BUDGET, prep).await else {
        return Ok(false);
    };
    // Focus first, like the click the mouse path does, so a following
    // `key Enter` lands in this field.
    if let Some(c) = comp {
        ctx.check()?;
        let _ = tokio::time::timeout(CHECK_BUDGET, c.grab_focus()).await;
    }
    ctx.check()?;
    let set = match tokio::time::timeout(ACT_BUDGET, edit.set_text_contents(text)).await {
        Ok(Ok(ok)) => ok,
        Ok(Err(e)) => {
            let _ = e;
            log::debug!("a11y set_text_contents failed");
            false
        }
        Err(_) => anyhow::bail!("accessibility text entry timed out; its effect is unknown"),
    };
    let text_now = || async {
        tokio::time::timeout(CHECK_BUDGET, read.get_text(0, -1))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default()
    };
    let mut now = text_now().await;
    // Some apps apply the edit a moment later (out-of-process content).
    // Wait briefly before concluding nothing happened, so the keyboard
    // fallback never types on top of a late edit.
    for _ in 0..SETTLE_POLLS {
        if !now.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
        now = text_now().await;
    }
    let mut set = set;
    if now.is_empty() {
        // Firefox reports success for SetTextContents on web inputs but
        // ignores it; InsertText at offset 0 of an empty field is the same edit.
        let n = text.chars().count() as i32;
        ctx.check()?;
        set = matches!(
            tokio::time::timeout(ACT_BUDGET, edit.insert_text(0, text, n)).await,
            Ok(Ok(true))
        );
        for _ in 0..SETTLE_POLLS {
            now = text_now().await;
            if !now.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    }
    if set && now == text {
        return Ok(true);
    }
    log::debug!("a11y fill could not verify the field");
    if now.is_empty() {
        return Ok(false); // nothing happened; the keyboard can do it cleanly
    }
    // Partly applied (e.g. a field that filters input): undo so the
    // keyboard fallback does not type twice.
    ctx.check()?;
    let _ = tokio::time::timeout(CHECK_BUDGET, edit.set_text_contents("")).await;
    let after = read.get_text(0, -1).await.unwrap_or_default();
    if after.is_empty() {
        Ok(false)
    } else {
        anyhow::bail!("accessibility text entry could not be verified")
    }
}

/// The toolkit of the app that owns `p` ("GTK", "Gecko", "VCL", "Qt").
async fn toolkit(
    p: &atspi::proxy::accessible::AccessibleProxy<'_>,
    bus: &zbus::Connection,
) -> Option<String> {
    let app = p.get_application().await.ok()?;
    let ap = app.as_accessible_proxy(bus).await.ok()?;
    let app = ap.proxies().await.ok()?.application().await.ok()?;
    app.toolkit_name().await.ok()
}

/// Alive, shown, enabled and where the observation showed it, right now
/// (states and places can change after observing).
async fn usable(p: &atspi::proxy::accessible::AccessibleProxy<'_>, seen: Rect) -> Option<bool> {
    let s = p.get_state().await.ok()?;
    let ok = !s.contains(State::Defunct)
        && s.contains(State::Showing)
        && (s.contains(State::Enabled) || s.contains(State::Sensitive));
    if ok && !same_place(seen, extents(p).await) {
        log::debug!("a11y: the element moved since the observation; using the mouse");
        return Some(false);
    }
    Some(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn picks_preferred_action_by_name() {
        let offered = names(&["activate", "Click", "press"]);
        assert_eq!(pick_action(&offered, press_names(Role::Button)), Some(1));
        let offered = names(&["click", "toggle"]);
        assert_eq!(pick_action(&offered, press_names(Role::Checkbox)), Some(1));
        let offered = names(&["jump"]);
        assert_eq!(pick_action(&offered, press_names(Role::Link)), Some(0));
    }

    #[test]
    fn no_matching_action_means_fallback() {
        assert_eq!(
            pick_action(&names(&["activate"]), press_names(Role::Checkbox)),
            None
        );
        assert_eq!(
            pick_action(&names(&["click"]), press_names(Role::ListItem)),
            None
        );
        assert_eq!(pick_action(&[], press_names(Role::Button)), None);
    }
}
