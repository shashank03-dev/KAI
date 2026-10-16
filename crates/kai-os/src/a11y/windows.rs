//! Windows: one cached UI Automation query over the foreground window and the
//! taskbars (one per display). The cache request brings every property back in a single
//! cross-process call, which keeps big apps fast on low-end machines.

use super::RawNode;
use kai_core::{Rect, Role};
use uiautomation::types::{ControlType, Handle, TreeScope, UIProperty};
use uiautomation::{UIAutomation, UIElement};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};
use windows::core::w;

const MAX_NODES: usize = 3000;
/// Taskbars read: the primary one plus one per other display.
const MAX_TASKBARS: usize = 6;

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
        UIProperty::HasKeyboardFocus,
        UIProperty::ValueValue,
    ] {
        cache.add_property(p)?;
    }
    let all = auto.create_true_condition()?;

    let mut roots: Vec<UIElement> = Vec::new();
    // SAFETY: plain Win32 queries with no pointers held past the call.
    let fg = unsafe { GetForegroundWindow() };
    // Kai's own window in front (the user just pressed Run): it is painted
    // out of the frame, so its controls must not become marks either.
    let mut owner = 0u32;
    // SAFETY: writes one u32 into `owner`.
    unsafe { GetWindowThreadProcessId(fg, Some(&mut owner)) };
    let fg = if owner == std::process::id() {
        Default::default()
    } else {
        fg
    };
    if !fg.is_invalid() {
        roots.push(auto.element_from_handle(Handle::from(fg.0 as isize))?);
    }
    // The title for the `window_title` probe (docs/VERIFICATION.md).
    let mut buf = [0u16; 512];
    // SAFETY: writes at most `buf.len()` UTF-16 units into our buffer.
    let n = if fg.is_invalid() {
        0
    } else {
        unsafe { GetWindowTextW(fg, &mut buf) }
    };
    let title = (n > 0).then(|| String::from_utf16_lossy(&buf[..n as usize]));
    super::active::set(title, None);
    // The taskbar on every display: `Shell_TrayWnd` on the primary one, a
    // `Shell_SecondaryTrayWnd` on each other one.
    // SAFETY: plain Win32 queries; `after` is a handle the last one returned.
    let mut bars: Vec<_> = unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }
        .into_iter()
        .collect();
    let mut after = None;
    while bars.len() < MAX_TASKBARS {
        let Ok(bar) = (unsafe { FindWindowExW(None, after, w!("Shell_SecondaryTrayWnd"), None) })
        else {
            break;
        };
        bars.push(bar);
        after = Some(bar);
    }
    for bar in bars {
        if !bar.is_invalid()
            && let Ok(el) = auto.element_from_handle(Handle::from(bar.0 as isize))
        {
            roots.push(el);
        }
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
            let Some(rect) = el.get_cached_bounding_rectangle().ok().and_then(bounds) else {
                continue;
            };
            let secure = el.is_cached_password().unwrap_or(false);
            // Read only for text fields and never for a password box. A
            // provider without the Value pattern reports a "not supported"
            // object, and a numeric value converts to text.
            let value = (role == Role::TextField && !secure)
                .then(|| el.get_cached_property_value(UIProperty::ValueValue).ok())
                .flatten()
                .and_then(|v| TryInto::<String>::try_into(v).ok());
            out.push(RawNode {
                role: if secure { Role::PasswordField } else { role },
                name: el.get_cached_name().unwrap_or_default(),
                bounds: rect,
                enabled: el.is_cached_enabled().unwrap_or(true),
                visible: !el.is_cached_offscreen().unwrap_or(false),
                secure,
                focused: el.has_cached_keyboard_focus().unwrap_or(false),
                value: super::field_value(role, secure, value),
                handle: Some(UiaRef(el.clone())),
            });
        }
    }
    Ok(out)
}

/// A UIA rectangle as Kai's, `None` when it is empty.
pub(super) fn bounds(r: uiautomation::types::Rect) -> Option<Rect> {
    let (l, t) = (r.get_left(), r.get_top());
    let (w, h) = (r.get_right() - l, r.get_bottom() - t);
    (w > 0 && h > 0).then(|| Rect::new(l as f32, t as f32, w as f32, h as f32))
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
