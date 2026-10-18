//! Linux: walk AT-SPI2 for the active window and any desktop panels.

use super::RawNode;
use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::{AccessibilityConnection, CoordType, ObjectRefOwned, Role as A, State};
use kai_core::{Rect, Role};
use std::sync::OnceLock;
use tokio::sync::OnceCell;

/// Most nodes visited per read; deep web pages are cut here.
const MAX_VISITS: usize = 2500;
const MAX_DEPTH: usize = 40;
/// A descendant-managing container with more children than this is not
/// walked (LibreOffice Calc's sheet reports 2^34 cells).
const MAX_MANAGED_CHILDREN: i32 = 500;
/// Apps whose top level is a panel/taskbar rather than a window.
const PANELS: &[&str] = &[
    "gnome-shell",
    "plasmashell",
    "xfce4-panel",
    "mate-panel",
    "lxpanel",
    "cinnamon",
    "budgie-panel",
    "waybar",
];

static CONN: OnceCell<AccessibilityConnection> = OnceCell::const_new();
static ENABLED: OnceLock<()> = OnceLock::new();

pub(super) async fn conn() -> anyhow::Result<&'static AccessibilityConnection> {
    if ENABLED.get().is_none() {
        // Asks GTK/Qt apps to expose their trees; harmless if already on.
        if let Err(e) = atspi::connection::set_session_accessibility(true).await {
            log::warn!("could not enable session accessibility: {e}");
        }
        let _ = ENABLED.set(());
    }
    Ok(CONN.get_or_try_init(AccessibilityConnection::new).await?)
}

pub async fn read(deadline: std::time::Instant) -> anyhow::Result<Vec<RawNode>> {
    let a11y = conn().await?;
    let bus = a11y.connection();
    let root = a11y.root_accessible_on_registry().await?;

    let mut roots: Vec<ObjectRefOwned> = Vec::new();
    let mut active: (Option<String>, Option<String>) = (None, None);
    let mut active_focused: Option<bool> = None;
    let own = std::process::id();
    let dbus = zbus::fdo::DBusProxy::new(bus).await.ok();
    for app in root.get_children().await? {
        // Kai's own windows are painted out of the frame: never marks.
        if let (Some(dbus), Some(name)) = (&dbus, app.name_as_str())
            && let Ok(name) = zbus::names::BusName::try_from(name)
            && dbus.get_connection_unix_process_id(name).await.ok() == Some(own)
        {
            continue;
        }
        let Ok(app_proxy) = app.as_accessible_proxy(bus).await else {
            continue;
        };
        let app_name = app_proxy
            .name()
            .await
            .unwrap_or_default()
            .to_ascii_lowercase();
        let panel = PANELS.iter().any(|p| app_name.contains(p));
        for win in app_proxy.get_children().await.unwrap_or_default() {
            let Ok(wp) = win.as_accessible_proxy(bus).await else {
                continue;
            };
            let state = wp.get_state().await.unwrap_or_default();
            let focused = state.contains(State::Focused);
            if state.contains(State::Active) && !panel && better_active(active_focused, focused) {
                active = (
                    Some(wp.name().await.unwrap_or_default()),
                    win.name_as_str().map(str::to_string),
                );
                active_focused = Some(focused);
            }
            if state.contains(State::Active) || panel {
                roots.push(win);
            }
        }
    }

    super::active::set(active.0, active.1);

    let mut out = Vec::new();
    let mut visits = 0usize;
    let mut stack: Vec<(ObjectRefOwned, usize)> = roots.into_iter().map(|r| (r, 0)).collect();
    while let Some((obj, depth)) = stack.pop() {
        visits += 1;
        if visits > MAX_VISITS {
            log::warn!(
                "accessibility read stopped after {MAX_VISITS} nodes; {} marks kept, the rest of the window is unread",
                out.len()
            );
            break;
        }
        // One node is several D-Bus calls; on a busy machine they alone can
        // outlast the budget, so each visit is bounded by the deadline too.
        let deadline = tokio::time::Instant::from_std(deadline);
        let Ok(seen) = tokio::time::timeout_at(deadline, visit(bus, &obj, depth)).await else {
            log::warn!(
                "accessibility read reached its time budget; {} marks kept, the rest of the window is unread",
                out.len()
            );
            break;
        };
        let Some((node, children)) = seen else {
            continue;
        };
        out.extend(node);
        stack.extend(children.into_iter().map(|c| (c, depth + 1)));
    }
    Ok(out)
}

/// One node: its mark (if it is one) and the children to read next; `None`
/// for a node that is gone or hidden (nothing under it is on screen).
async fn visit(
    bus: &zbus::Connection,
    obj: &ObjectRefOwned,
    depth: usize,
) -> Option<(Option<RawNode>, Vec<ObjectRefOwned>)> {
    let p = obj.as_accessible_proxy(bus).await.ok()?;
    let state = p.get_state().await.unwrap_or_default();
    if !state.contains(State::Showing) {
        return None;
    }
    let arole = p.get_role().await.unwrap_or(A::Invalid);
    let role = map_role(arole);
    let mut node = None;
    if role != Role::Other
        && let Some(bounds) = extents(&p).await
    {
        let name = label(&p).await;
        // A web table cell or list item has no name; its text is what
        // it says (seen live: Firefox opening hours read as `cell ""`,
        // and the model misread the screenshot).
        let shows_text = matches!(role, Role::Cell | Role::ListItem) && name.trim().is_empty();
        node = Some(RawNode {
            role,
            name,
            bounds,
            enabled: state.contains(State::Enabled) || state.contains(State::Sensitive),
            visible: state.contains(State::Visible),
            secure: arole == A::PasswordText,
            focused: state.contains(State::Focused),
            value: if role == Role::TextField && arole != A::Terminal {
                field_text(&p).await
            } else if shows_text {
                field_text(&p).await.and_then(|t| content_text(&t))
            } else {
                None
            },
            handle: Some(obj.clone()),
        });
    }
    // Containers that manage their descendants (a spreadsheet grid, huge
    // lists) report millions of children; enumerating them stalls the
    // read past its budget, so they are kept as one node.
    let manages = state.contains(State::ManagesDescendants);
    let n = p.child_count().await.unwrap_or(0);
    let children = if depth < MAX_DEPTH && n > 0 && !(manages && n > MAX_MANAGED_CHILDREN) {
        p.get_children().await.unwrap_or_default()
    } else {
        Vec::new()
    };
    Some((node, children))
}

/// Several windows can report `Active` (seen with dialogs and some
/// toolkits): the first one wins unless a later one also has focus, so the
/// title does not depend on bus order alone. `have` is whether the window
/// picked so far had focus (`None`: nothing picked yet).
fn better_active(have: Option<bool>, focused: bool) -> bool {
    match have {
        None => true,
        Some(had) => focused && !had,
    }
}

pub(super) async fn extents(p: &AccessibleProxy<'_>) -> Option<Rect> {
    let comp = p.proxies().await.ok()?.component().await.ok()?;
    let (x, y, w, h) = comp.get_extents(CoordType::Screen).await.ok()?;
    (w > 0 && h > 0).then(|| Rect::new(x as f32, y as f32, w as f32, h as f32))
}

/// A text field's contents (first 200 characters), so the model can see what
/// it typed. Never called for password fields (their role differs).
async fn field_text(p: &AccessibleProxy<'_>) -> Option<String> {
    let text = p.proxies().await.ok()?.text().await.ok()?;
    let n = text.character_count().await.ok()?;
    if n <= 0 {
        return None;
    }
    let t = text.get_text(0, n.min(200)).await.ok()?;
    (!t.trim().is_empty()).then_some(t)
}

/// The readable part of an element's text: embedded-object marks (U+FFFC,
/// where a child element sits) dropped, whitespace collapsed; `None` when
/// nothing is left.
fn content_text(raw: &str) -> Option<String> {
    let t = raw
        .replace('\u{fffc}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!t.is_empty()).then_some(t)
}

async fn label(p: &AccessibleProxy<'_>) -> String {
    let name = p.name().await.unwrap_or_default();
    if !name.trim().is_empty() {
        return name;
    }
    p.description().await.unwrap_or_default()
}

pub(super) fn map_role(r: A) -> Role {
    match r {
        A::Button | A::ToggleButton | A::PushButtonMenu => Role::Button,
        A::Link => Role::Link,
        A::Entry | A::Text | A::Terminal | A::SpinButton | A::Autocomplete | A::Editbar => {
            Role::TextField
        }
        A::PasswordText => Role::PasswordField,
        A::CheckBox | A::CheckMenuItem => Role::Checkbox,
        A::RadioButton | A::RadioMenuItem => Role::Radio,
        A::MenuItem | A::Menu => Role::MenuItem,
        A::PageTab => Role::Tab,
        A::ComboBox => Role::ComboBox,
        A::Slider | A::ScrollBar => Role::Slider,
        A::ListItem | A::TreeItem => Role::ListItem,
        A::TableCell => Role::Cell,
        A::Icon | A::DesktopIcon => Role::Button,
        _ => Role::Other,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_focused_active_window_beats_an_earlier_unfocused_one() {
        use super::better_active;
        assert!(better_active(None, false));
        assert!(better_active(Some(false), true));
        assert!(!better_active(Some(false), false), "first one wins");
        assert!(!better_active(Some(true), true));
    }

    #[test]
    fn a_cells_text_drops_child_marks_and_extra_space() {
        assert_eq!(
            super::content_text("8:00 - 13:30").as_deref(),
            Some("8:00 - 13:30")
        );
        assert_eq!(
            super::content_text("\u{fffc} Saturday\n  hours").as_deref(),
            Some("Saturday hours")
        );
        assert_eq!(super::content_text("\u{fffc}"), None);
        assert_eq!(super::content_text("  "), None);
    }
}
