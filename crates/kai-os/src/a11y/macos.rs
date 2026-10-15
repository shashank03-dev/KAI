//! macOS: walk the Accessibility API (`AXUIElement`) for the frontmost app's
//! focused window and its menu bar. Each element's attributes come back in
//! one cross-process call (`AXUIElementCopyMultipleAttributeValues`), and a
//! messaging timeout keeps a hung app from stalling the read. Needs Kai to
//! be allowed under System Settings › Privacy & Security › Accessibility.

use super::RawNode;
use super::macos_roles::{child_clip, in_clip, map_role, pick_name};
use kai_core::{Rect, Role};
use objc2_app_kit::NSWorkspace;
use objc2_application_services::{
    AXCopyMultipleAttributeOptions, AXError, AXUIElement, AXValue, AXValueType,
};
use objc2_core_foundation::{CFArray, CFBoolean, CFRetained, CFString, CFType, CGPoint, CGSize};
use objc2_foundation::{NSNumber, NSString};
use std::ptr::NonNull;
use std::time::Instant;

const MAX_NODES: usize = 3000;
const MAX_DEPTH: usize = 40;
/// Seconds one call may wait on the target app before it fails.
const MESSAGING_TIMEOUT: f32 = 0.25;

/// The attributes read per element; the `A_*` indices below follow it.
const ATTRS: [&str; 12] = [
    "AXRole",
    "AXSubrole",
    "AXTitle",
    "AXDescription",
    "AXPlaceholderValue",
    "AXValue",
    "AXPosition",
    "AXSize",
    "AXEnabled",
    "AXFocused",
    "AXSelected",
    "AXChildren",
];
const A_ROLE: usize = 0;
const A_SUBROLE: usize = 1;
const A_TITLE: usize = 2;
const A_DESCRIPTION: usize = 3;
const A_PLACEHOLDER: usize = 4;
const A_VALUE: usize = 5;
const A_POSITION: usize = 6;
const A_SIZE: usize = 7;
const A_ENABLED: usize = 8;
const A_FOCUSED: usize = 9;
const A_SELECTED: usize = 10;
const A_CHILDREN: usize = 11;

/// A live accessibility element kept for accessibility-first actions.
#[derive(Clone)]
pub struct AxRef(pub CFRetained<AXUIElement>);

// SAFETY: an AXUIElementRef is an immutable CoreFoundation object (retain
// and release are atomic), and the AX client API may be called from any
// thread. Kai only calls it on blocking-pool threads.
unsafe impl Send for AxRef {}
unsafe impl Sync for AxRef {}

impl std::fmt::Debug for AxRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AxRef")
    }
}

pub fn read() -> anyhow::Result<Vec<RawNode>> {
    let deadline = Instant::now() + super::READ_BUDGET;
    // SAFETY: creating the system-wide element has no preconditions. A
    // timeout set on it applies to every element (Apple's documentation),
    // so no single call can wait on a hung app for long.
    let system = unsafe { AXUIElement::new_system_wide() };
    unsafe { system.set_messaging_timeout(MESSAGING_TIMEOUT) };
    let app = frontmost_pid()
        // SAFETY: creating an app element from a pid has no preconditions.
        .map(|pid| unsafe { AXUIElement::new_application(pid) })
        .or_else(|| attr(&system, "AXFocusedApplication").and_then(element))
        .ok_or_else(|| anyhow::anyhow!("no frontmost app to read"))?;
    if !trusted() {
        anyhow::bail!(
            "not allowed to read other apps: allow Kai under System Settings › \
             Privacy & Security › Accessibility"
        );
    }
    // SAFETY: a plain setter on a live element. Electron apps (Slack, VS
    // Code, Discord) build their tree only once a client sets
    // `AXManualAccessibility`; other apps ignore it.
    unsafe {
        app.set_attribute_value(
            &CFString::from_static_str("AXManualAccessibility"),
            CFBoolean::new(true),
        );
    }
    let window = attr(&app, "AXFocusedWindow").and_then(element);
    let title = window
        .as_ref()
        .and_then(|w| attr(w, "AXTitle"))
        .and_then(|v| string(&v));
    super::active::set(title, None);

    let names = CFArray::from_retained_objects(&ATTRS.map(CFString::from_static_str));
    let mut walk = Walk {
        names: names.as_opaque(),
        deadline,
        out: Vec::new(),
    };
    if let Some(w) = window {
        walk.run(w);
    }
    if let Some(bar) = attr(&app, "AXMenuBar").and_then(element) {
        walk.run(bar);
    }
    Ok(walk.out)
}

/// The frontmost app's process id, read now. `activeApplication` is
/// deprecated, but `frontmostApplication` only updates in a process that
/// runs an AppKit event loop, which the `kai` command line does not (xcap
/// reads it the same way). The system-wide `AXFocusedApplication` was seen
/// failing on a Mac where Kai was trusted (GitHub's macos-15, 2026-10-15).
#[allow(deprecated)]
fn frontmost_pid() -> Option<i32> {
    let key = NSString::from_str("NSApplicationProcessIdentifier");
    let app = NSWorkspace::sharedWorkspace().activeApplication()?;
    let pid = app.valueForKey(&key)?.downcast::<NSNumber>().ok()?;
    Some(pid.intValue())
}

fn trusted() -> bool {
    // SAFETY: no arguments, no preconditions.
    unsafe { objc2_application_services::AXIsProcessTrusted() }
}

struct Walk<'a> {
    names: &'a CFArray,
    deadline: Instant,
    out: Vec<RawNode>,
}

impl Walk<'_> {
    /// Depth-first from `root`. A node scrolled out of its scroll area is
    /// kept but marked invisible; menus and popovers, which can reach past
    /// their window, are not clipped.
    fn run(&mut self, root: CFRetained<AXUIElement>) {
        let mut stack: Vec<(CFRetained<AXUIElement>, usize, Option<Rect>)> = vec![(root, 0, None)];
        while let Some((el, depth, clip)) = stack.pop() {
            if self.out.len() >= MAX_NODES || Instant::now() > self.deadline {
                return;
            }
            let Some(v) = values(&el, self.names) else {
                continue;
            };
            let get = |i: usize| v.get(i);
            let text = |i: usize| get(i).and_then(|x| string(&x)).unwrap_or_default();
            let flag = |i: usize| get(i).and_then(|x| boolean(&x));
            let (ax_role, subrole) = (text(A_ROLE), text(A_SUBROLE));
            let role = map_role(&ax_role, &subrole);
            let bounds = rect(get(A_POSITION), get(A_SIZE));
            if role != Role::Other
                && let Some(bounds) = bounds
            {
                let secure = role == Role::PasswordField;
                self.out.push(RawNode {
                    role,
                    name: pick_name(&text(A_TITLE), &text(A_DESCRIPTION), &text(A_PLACEHOLDER)),
                    bounds,
                    enabled: flag(A_ENABLED).unwrap_or(true),
                    visible: in_clip(bounds, clip),
                    secure,
                    focused: flag(A_FOCUSED).unwrap_or(false),
                    value: (role == Role::TextField).then(|| text(A_VALUE)),
                    handle: Some(AxRef(el.clone())),
                });
            }
            // A closed menu's items exist in the tree but are not on screen.
            let closed_menu = ax_role == "AXMenuBarItem" && flag(A_SELECTED) != Some(true);
            if depth >= MAX_DEPTH || closed_menu {
                continue;
            }
            let kids = get(A_CHILDREN).map(|c| children(&c)).unwrap_or_default();
            let clip = child_clip(clip, &ax_role, bounds);
            stack.extend(kids.into_iter().rev().map(|k| (k, depth + 1, clip)));
        }
    }
}

/// One attribute of `el`, or `None` when the app does not report it.
pub(super) fn attr(el: &AXUIElement, name: &'static str) -> Option<CFRetained<CFType>> {
    let mut out: *const CFType = std::ptr::null();
    // SAFETY: `out` is a valid out pointer; on success it holds a +1
    // reference that `from_raw` takes over.
    let err = unsafe {
        el.copy_attribute_value(&CFString::from_static_str(name), NonNull::from(&mut out))
    };
    if err != AXError::Success {
        return None;
    }
    NonNull::new(out.cast_mut()).map(|p| unsafe { CFRetained::from_raw(p) })
}

/// All of [`ATTRS`] for `el` in one call. A missing attribute comes back as
/// an `AXValue` holding an error, which the readers below turn into `None`.
fn values(el: &AXUIElement, names: &CFArray) -> Option<CFRetained<CFArray<CFType>>> {
    let mut out: *const CFArray = std::ptr::null();
    // SAFETY: `names` holds CFStrings and `out` is a valid out pointer; on
    // success it holds a +1 reference to an array of CF objects.
    let err = unsafe {
        el.copy_multiple_attribute_values(
            names,
            AXCopyMultipleAttributeOptions::empty(),
            NonNull::from(&mut out),
        )
    };
    if err != AXError::Success {
        return None;
    }
    let arr = unsafe { CFRetained::from_raw(NonNull::new(out.cast_mut())?) };
    // SAFETY: every element of the array is a CF object.
    Some(unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(arr) })
}

fn element(v: CFRetained<CFType>) -> Option<CFRetained<AXUIElement>> {
    v.downcast::<AXUIElement>().ok()
}

fn children(v: &CFType) -> Vec<CFRetained<AXUIElement>> {
    let Some(arr) = v.downcast_ref::<CFArray>() else {
        return Vec::new();
    };
    // SAFETY: every element of a CFArray is a CF object.
    let arr = unsafe { arr.cast_unchecked::<CFType>() };
    arr.iter().filter_map(element).collect()
}

pub(super) fn string(v: &CFType) -> Option<String> {
    v.downcast_ref::<CFString>().map(|s| s.to_string())
}

pub(super) fn boolean(v: &CFType) -> Option<bool> {
    v.downcast_ref::<CFBoolean>().map(CFBoolean::as_bool)
}

/// Read a `CGPoint` or `CGSize` out of an `AXValue`; `None` for any other
/// value (including the error values of missing attributes).
fn ax_value<T>(v: &CFType, kind: AXValueType, mut out: T) -> Option<T> {
    let ax = v.downcast_ref::<AXValue>()?;
    // SAFETY: `value` checks the stored type against `kind` and only then
    // writes a `T`-sized struct into `out`.
    unsafe { ax.value(kind, NonNull::from(&mut out).cast()) }.then_some(out)
}

fn rect(pos: Option<CFRetained<CFType>>, size: Option<CFRetained<CFType>>) -> Option<Rect> {
    let p = ax_value(&*pos?, AXValueType::CGPoint, CGPoint { x: 0.0, y: 0.0 })?;
    let s = ax_value(
        &*size?,
        AXValueType::CGSize,
        CGSize {
            width: 0.0,
            height: 0.0,
        },
    )?;
    (s.width > 0.0 && s.height > 0.0)
        .then(|| Rect::new(p.x as f32, p.y as f32, s.width as f32, s.height as f32))
}
