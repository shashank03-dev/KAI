//! Accessibility trees → Set-of-Marks. Each OS reads raw nodes (Windows UI
//! Automation, Linux AT-SPI, the macOS Accessibility API); this module keeps the visible, interactive ones,
//! numbers them in reading order, and falls back to a grid when the tree is
//! empty or useless (games, canvases, some Electron apps).

pub mod active;
#[cfg(target_os = "linux")]
pub mod events;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod linux_act;
pub mod live;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_act;
#[cfg(any(target_os = "macos", test))]
mod macos_roles;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
mod windows_act;

use kai_core::{Element, Grounding, Rect, Role};
use std::time::{Duration, Instant};

/// A handle back to the live accessible object a node came from: bus name +
/// object path on AT-SPI, the UI Automation element on Windows, the
/// `AXUIElement` on macOS.
#[cfg(target_os = "linux")]
pub type NodeRef = atspi::ObjectRefOwned;
#[cfg(target_os = "windows")]
pub type NodeRef = windows::UiaRef;
#[cfg(target_os = "macos")]
pub type NodeRef = macos::AxRef;
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub type NodeRef = ();

/// One node as the OS reported it.
#[derive(Debug, Clone)]
pub struct RawNode {
    pub role: Role,
    pub name: String,
    pub bounds: Rect,
    pub enabled: bool,
    pub visible: bool,
    pub secure: bool,
    /// Has keyboard focus (AT-SPI `Focused`, UIA `HasKeyboardFocus`).
    pub focused: bool,
    /// Current text of a field (AT-SPI Text, `AXValue`, UIA `ValueValue`),
    /// see [`field_value`]. `None` when unknown.
    pub value: Option<String>,
    /// The live object, for accessibility-first actions. `None` in tests.
    pub handle: Option<NodeRef>,
}

/// Budget for one tree read; a slow app yields what was read so far.
pub const READ_BUDGET: Duration = Duration::from_millis(600);
/// How long past the budget a reader may take to hand back its partial
/// tree (one in-flight call) before the read is abandoned.
const READ_GRACE: Duration = Duration::from_millis(400);
/// Most marks drawn on one screen; past this the image is noise.
pub const MAX_MARKS: usize = 150;
/// Fewer interactive elements than this means the tree is not useful.
pub const MIN_USEFUL: usize = 2;

/// Read the focused window's tree plus the taskbar/panel, within the budget.
pub async fn read_tree() -> Vec<RawNode> {
    active::clear_title();
    within_budget(READ_BUDGET, platform_read).await
}

/// Run `read` with a deadline `budget` from now. Readers stop at the
/// deadline and return what they read; the timeout only catches one that
/// cannot (a call hung past the grace). Seen live: a GTK save dialog on a
/// busy CPU overran the budget, the whole tree was dropped for the grid,
/// and the model lost its place.
async fn within_budget<F>(budget: Duration, read: impl FnOnce(Instant) -> F) -> Vec<RawNode>
where
    F: std::future::Future<Output = anyhow::Result<Vec<RawNode>>>,
{
    let limit = budget + READ_GRACE;
    match tokio::time::timeout(limit, read(Instant::now() + budget)).await {
        Ok(Ok(nodes)) => nodes,
        Ok(Err(e)) => {
            log::warn!("accessibility tree unavailable: {e}");
            Vec::new()
        }
        Err(_) => {
            log::warn!("accessibility tree read hung past {limit:?}; using grid");
            Vec::new()
        }
    }
}

#[cfg(target_os = "linux")]
async fn platform_read(deadline: Instant) -> anyhow::Result<Vec<RawNode>> {
    linux::read(deadline).await
}

/// UI Automation reads the tree in one cached query, which cannot stop
/// halfway: the timeout above bounds it.
#[cfg(target_os = "windows")]
async fn platform_read(_deadline: Instant) -> anyhow::Result<Vec<RawNode>> {
    tokio::task::spawn_blocking(windows::read).await?
}

#[cfg(target_os = "macos")]
async fn platform_read(deadline: Instant) -> anyhow::Result<Vec<RawNode>> {
    tokio::task::spawn_blocking(move || macos::read(deadline)).await?
}

/// The focused window of the app in front (macOS), for picking the display
/// to capture.
#[cfg(target_os = "macos")]
pub(crate) fn focused_window_rect() -> Option<Rect> {
    macos::focused_window_rect()
}

/// The bundle id of the app in front (macOS).
#[cfg(target_os = "macos")]
pub(crate) fn frontmost_app_id() -> Option<String> {
    macos::frontmost_app_id()
}

/// A running count of accessibility events from the active app (Linux,
/// AT-SPI); `None` where events are not wired (Windows) or not running.
pub fn ui_events() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        events::start();
        events::count()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
async fn platform_read(_deadline: Instant) -> anyhow::Result<Vec<RawNode>> {
    anyhow::bail!("no accessibility reader on this OS; using the grid")
}

/// Most characters of a field's text kept in the element list.
pub const MAX_VALUE_CHARS: usize = 200;

/// What the element list shows as a field's text: only for text fields,
/// never for secure ones, the first [`MAX_VALUE_CHARS`] characters, `None`
/// when blank.
pub fn field_value(role: Role, secure: bool, text: Option<String>) -> Option<String> {
    if role != Role::TextField || secure {
        return None;
    }
    let text = text?;
    if text.trim().is_empty() {
        return None;
    }
    Some(text.chars().take(MAX_VALUE_CHARS).collect())
}

fn interactive(role: Role) -> bool {
    !matches!(role, Role::Other | Role::Image)
}

/// Perform a decision through the element's accessibility interface. `Ok(true)`
/// when done that way; `Ok(false)` whenever anything is uncertain (no live
/// handle for this mark, element gone, disabled, secure, no matching action,
/// field not empty) so the caller falls back to mouse and keyboard.
pub async fn act(decision: &kai_core::Decision, grounding: &Grounding) -> anyhow::Result<bool> {
    act_context(
        decision,
        grounding,
        &kai_core::computer::ExecutionContext::new(std::time::Duration::from_secs(30)),
    )
    .await
}

pub async fn act_context(
    decision: &kai_core::Decision,
    grounding: &Grounding,
    context: &kai_core::computer::ExecutionContext,
) -> anyhow::Result<bool> {
    context.check()?;
    let Some((el, plan)) = live::resolve(decision, grounding) else {
        return Ok(false);
    };
    let Some(node) = live::lookup(&el, grounding) else {
        return Ok(false);
    };
    platform_act(node, plan, el, context.clone()).await
}

#[cfg(target_os = "linux")]
async fn platform_act(
    node: NodeRef,
    plan: live::Plan,
    el: Element,
    context: kai_core::computer::ExecutionContext,
) -> anyhow::Result<bool> {
    linux_act::act(&node, &plan, &el, &context).await
}

#[cfg(target_os = "windows")]
async fn platform_act(
    node: NodeRef,
    plan: live::Plan,
    el: Element,
    context: kai_core::computer::ExecutionContext,
) -> anyhow::Result<bool> {
    tokio::task::spawn_blocking(move || windows_act::act(&node, &plan, &el, &context)).await?
}

#[cfg(target_os = "macos")]
async fn platform_act(
    node: NodeRef,
    plan: live::Plan,
    el: Element,
    context: kai_core::computer::ExecutionContext,
) -> anyhow::Result<bool> {
    tokio::task::spawn_blocking(move || macos_act::act(&node, &plan, &el, &context)).await?
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
async fn platform_act(
    _: NodeRef,
    _: live::Plan,
    _: Element,
    _: kai_core::computer::ExecutionContext,
) -> anyhow::Result<bool> {
    Ok(false)
}

/// Turn raw nodes into numbered marks, or a grid if there is nothing useful.
pub fn ground(nodes: &[RawNode], screen: Rect) -> Grounding {
    ground_indexed(nodes, screen).0
}

/// Like [`ground`], and remember which live object each mark is, so a later
/// [`act`] on this grounding can reach it.
pub fn ground_live(nodes: &[RawNode], screen: Rect) -> Grounding {
    let (g, picked) = ground_indexed(nodes, screen);
    live::remember(&g, nodes, &picked);
    g
}

/// The grounding plus, for each mark in order, the index of its raw node
/// (the browser lane maps that back to a DOM node).
pub fn ground_indexed(nodes: &[RawNode], screen: Rect) -> (Grounding, Vec<usize>) {
    let keep: Vec<(usize, &RawNode)> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.visible && interactive(n.role))
        .filter(|(_, n)| n.bounds.w >= 4.0 && n.bounds.h >= 4.0)
        .filter(|(_, n)| on_screen(n.bounds, screen))
        .collect();

    let keep = reading_order(keep);

    // Drop exact duplicates (a button and its inner label share a box).
    let mut marks: Vec<Element> = Vec::new();
    let mut picked: Vec<usize> = Vec::new();
    for (i, n) in keep {
        if let Some(m) = marks.iter_mut().find(|m| near(m.bounds, n.bounds)) {
            // A focused or secure inner node marks the box that stands for it.
            m.focused |= n.focused;
            m.secure |= n.secure || n.role == Role::PasswordField;
            continue;
        }
        if marks.len() >= MAX_MARKS {
            break;
        }
        picked.push(i);
        marks.push(Element {
            mark: marks.len() as u32 + 1,
            role: n.role,
            name: clean(&n.name),
            bounds: n.bounds,
            enabled: n.enabled,
            secure: n.secure || n.role == Role::PasswordField,
            focused: n.focused,
            value: n.value.clone(),
        });
    }

    if marks.len() < MIN_USEFUL {
        return (grid_for(screen), Vec::new());
    }
    (Grounding::Marks(marks), picked)
}

/// A grid with roughly square cells, about 120px on a 1080p screen.
pub fn grid_for(screen: Rect) -> Grounding {
    let cols = ((screen.w / 120.0).round() as u32).clamp(4, 24);
    let rows = ((screen.h / 120.0).round() as u32).clamp(3, 16);
    Grounding::Grid { cols, rows, screen }
}

/// Top to bottom in lines, left to right within a line. An element joins the
/// current line when its vertical centre sits within half the line's height.
/// A raw node and its index in the read.
type Indexed<'a> = (usize, &'a RawNode);

fn reading_order(mut nodes: Vec<Indexed<'_>>) -> Vec<Indexed<'_>> {
    nodes.sort_by(|a, b| a.1.bounds.center().1.total_cmp(&b.1.bounds.center().1));
    let mut lines: Vec<(f32, f32, Vec<Indexed>)> = Vec::new();
    for (i, n) in nodes {
        let (_, cy) = n.bounds.center();
        match lines.last_mut() {
            Some((ly, lh, line)) if (cy - *ly).abs() <= (lh.min(n.bounds.h)) / 2.0 => {
                line.push((i, n))
            }
            _ => lines.push((cy, n.bounds.h, vec![(i, n)])),
        }
    }
    lines
        .into_iter()
        .flat_map(|(_, _, mut line)| {
            line.sort_by(|a, b| a.1.bounds.x.total_cmp(&b.1.bounds.x));
            line
        })
        .collect()
}

fn on_screen(r: Rect, s: Rect) -> bool {
    let (cx, cy) = r.center();
    cx >= s.x && cy >= s.y && cx < s.x + s.w && cy < s.y + s.h
}

fn near(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() < 2.0
        && (a.y - b.y).abs() < 2.0
        && (a.w - b.w).abs() < 2.0
        && (a.h - b.h).abs() < 2.0
}

fn clean(name: &str) -> String {
    let one_line: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut s: String = one_line.chars().take(60).collect();
    if one_line.chars().count() > 60 {
        s.push('…');
    }
    s.replace('"', "'")
}

#[cfg(test)]
mod tests;
