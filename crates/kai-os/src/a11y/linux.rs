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

async fn conn() -> anyhow::Result<&'static AccessibilityConnection> {
    if ENABLED.get().is_none() {
        // Asks GTK/Qt apps to expose their trees; harmless if already on.
        if let Err(e) = atspi::connection::set_session_accessibility(true).await {
            log::warn!("could not enable session accessibility: {e}");
        }
        let _ = ENABLED.set(());
    }
    Ok(CONN.get_or_try_init(AccessibilityConnection::new).await?)
}

pub async fn read() -> anyhow::Result<Vec<RawNode>> {
    let a11y = conn().await?;
    let bus = a11y.connection();
    let root = a11y.root_accessible_on_registry().await?;

    let mut roots: Vec<ObjectRefOwned> = Vec::new();
    for app in root.get_children().await? {
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
            if state.contains(State::Active) || panel {
                roots.push(win);
            }
        }
    }

    let mut out = Vec::new();
    let mut visits = 0usize;
    let mut stack: Vec<(ObjectRefOwned, usize)> = roots.into_iter().map(|r| (r, 0)).collect();
    while let Some((obj, depth)) = stack.pop() {
        visits += 1;
        if visits > MAX_VISITS {
            break;
        }
        let Ok(p) = obj.as_accessible_proxy(bus).await else {
            continue;
        };
        let state = p.get_state().await.unwrap_or_default();
        if !state.contains(State::Showing) {
            continue; // nothing under a hidden node is on screen
        }
        let arole = p.get_role().await.unwrap_or(A::Invalid);
        let role = map_role(arole);
        if role != Role::Other
            && let Some(bounds) = extents(&p).await
        {
            out.push(RawNode {
                role,
                name: label(&p).await,
                bounds,
                enabled: state.contains(State::Enabled) || state.contains(State::Sensitive),
                visible: state.contains(State::Visible),
                secure: arole == A::PasswordText,
            });
        }
        if depth < MAX_DEPTH && p.child_count().await.unwrap_or(0) > 0 {
            for c in p.get_children().await.unwrap_or_default() {
                stack.push((c, depth + 1));
            }
        }
    }
    Ok(out)
}

async fn extents(p: &AccessibleProxy<'_>) -> Option<Rect> {
    let comp = p.proxies().await.ok()?.component().await.ok()?;
    let (x, y, w, h) = comp.get_extents(CoordType::Screen).await.ok()?;
    (w > 0 && h > 0).then(|| Rect::new(x as f32, y as f32, w as f32, h as f32))
}

async fn label(p: &AccessibleProxy<'_>) -> String {
    let name = p.name().await.unwrap_or_default();
    if !name.trim().is_empty() {
        return name;
    }
    p.description().await.unwrap_or_default()
}

fn map_role(r: A) -> Role {
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
