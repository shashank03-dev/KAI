//! Windows: one cached UI Automation query over the foreground window and the
//! taskbar. The cache request brings every property back in a single
//! cross-process call, which keeps big apps fast on low-end machines.

use super::RawNode;
use kai_core::{Rect, Role};
use uiautomation::types::{ControlType, Handle, TreeScope, UIProperty};
use uiautomation::{UIAutomation, UIElement};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetForegroundWindow};
use windows::core::w;

const MAX_NODES: usize = 3000;

/// A live UI Automation element kept for accessibility-first actions.
#[derive(Clone)]
pub struct UiaRef(pub UIElement);

// SAFETY: `UIAutomation::new` joins the multithreaded apartment, and UI
// Automation client objects created in the MTA are free-threaded: any MTA
// thread may call them. Kai only touches them on blocking-pool threads that
// also call `UIAutomation::new` first (see `windows_act`).
unsafe impl Send for UiaRef {}
unsafe impl Sync for UiaRef {}

impl std::fmt::Debug for UiaRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UiaRef")
    }
}

pub fn read() -> anyhow::Result<Vec<RawNode>> {
    let auto = UIAutomation::new()?;
    let cache = auto.create_cache_request()?;
    for p in [
        UIProperty::Name,
        UIProperty::ControlType,
        UIProperty::BoundingRectangle,
        UIProperty::IsEnabled,
        UIProperty::IsOffscreen,
        UIProperty::IsPassword,
    ] {
        cache.add_property(p)?;
    }
    let all = auto.create_true_condition()?;

    let mut roots: Vec<UIElement> = Vec::new();
    // SAFETY: plain Win32 queries with no pointers held past the call.
    let fg = unsafe { GetForegroundWindow() };
    if !fg.is_invalid() {
        roots.push(auto.element_from_handle(Handle::from(fg.0 as isize))?);
    }
    if let Ok(tray) = unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }
        && !tray.is_invalid()
        && let Ok(el) = auto.element_from_handle(Handle::from(tray.0 as isize))
    {
        roots.push(el);
    }

    let mut out = Vec::new();
    for root in roots {
        for el in root
            .find_all_build_cache(TreeScope::Descendants, &all, &cache)?
            .into_iter()
            .take(MAX_NODES)
        {
            let Ok(ct) = el.get_cached_control_type() else {
                continue;
            };
            let role = map_role(ct);
            if role == Role::Other {
                continue;
            }
            let Ok(r) = el.get_cached_bounding_rectangle() else {
                continue;
            };
            let (l, t) = (r.get_left(), r.get_top());
            let (w, h) = (r.get_right() - l, r.get_bottom() - t);
            if w <= 0 || h <= 0 {
                continue;
            }
            let secure = el.is_cached_password().unwrap_or(false);
            out.push(RawNode {
                role: if secure { Role::PasswordField } else { role },
                name: el.get_cached_name().unwrap_or_default(),
                bounds: Rect::new(l as f32, t as f32, w as f32, h as f32),
                enabled: el.is_cached_enabled().unwrap_or(true),
                visible: !el.is_cached_offscreen().unwrap_or(false),
                secure,
                value: None,
                handle: Some(UiaRef(el.clone())),
            });
        }
    }
    Ok(out)
}

fn map_role(ct: ControlType) -> Role {
    match ct {
        ControlType::Button | ControlType::SplitButton => Role::Button,
        ControlType::Hyperlink => Role::Link,
        ControlType::Edit | ControlType::Document | ControlType::Spinner => Role::TextField,
        ControlType::CheckBox => Role::Checkbox,
        ControlType::RadioButton => Role::Radio,
        ControlType::MenuItem => Role::MenuItem,
        ControlType::TabItem => Role::Tab,
        ControlType::ComboBox => Role::ComboBox,
        ControlType::Slider | ControlType::ScrollBar => Role::Slider,
        ControlType::ListItem | ControlType::TreeItem | ControlType::DataItem => Role::ListItem,
        _ => Role::Other,
    }
}
