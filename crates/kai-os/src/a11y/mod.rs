//! Accessibility trees → Set-of-Marks. Each OS reads raw nodes (Windows UI
//! Automation, Linux AT-SPI); this module keeps the visible, interactive ones,
//! numbers them in reading order, and falls back to a grid when the tree is
//! empty or useless (games, canvases, some Electron apps).

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

use kai_core::{Element, Grounding, Rect, Role};
use std::time::Duration;

/// One node as the OS reported it.
#[derive(Debug, Clone)]
pub struct RawNode {
    pub role: Role,
    pub name: String,
    pub bounds: Rect,
    pub enabled: bool,
    pub visible: bool,
    pub secure: bool,
}

/// Budget for one tree read; a slow app yields what was read so far.
pub const READ_BUDGET: Duration = Duration::from_millis(600);
/// Most marks drawn on one screen; past this the image is noise.
pub const MAX_MARKS: usize = 150;
/// Fewer interactive elements than this means the tree is not useful.
pub const MIN_USEFUL: usize = 2;

/// Read the focused window's tree plus the taskbar/panel, within the budget.
pub async fn read_tree() -> Vec<RawNode> {
    let fut = platform_read();
    match tokio::time::timeout(READ_BUDGET, fut).await {
        Ok(Ok(nodes)) => nodes,
        Ok(Err(e)) => {
            log::warn!("accessibility tree unavailable: {e}");
            Vec::new()
        }
        Err(_) => {
            log::warn!("accessibility tree read exceeded {READ_BUDGET:?}; using grid");
            Vec::new()
        }
    }
}

#[cfg(target_os = "linux")]
async fn platform_read() -> anyhow::Result<Vec<RawNode>> {
    linux::read().await
}

#[cfg(target_os = "windows")]
async fn platform_read() -> anyhow::Result<Vec<RawNode>> {
    tokio::task::spawn_blocking(windows::read).await?
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
async fn platform_read() -> anyhow::Result<Vec<RawNode>> {
    anyhow::bail!("no accessibility reader on this OS; using the grid")
}

fn interactive(role: Role) -> bool {
    !matches!(role, Role::Other | Role::Image)
}

/// Turn raw nodes into numbered marks, or a grid if there is nothing useful.
pub fn ground(nodes: &[RawNode], screen: Rect) -> Grounding {
    let keep: Vec<&RawNode> = nodes
        .iter()
        .filter(|n| n.visible && interactive(n.role))
        .filter(|n| n.bounds.w >= 4.0 && n.bounds.h >= 4.0)
        .filter(|n| on_screen(n.bounds, screen))
        .collect();

    let keep = reading_order(keep);

    // Drop exact duplicates (a button and its inner label share a box).
    let mut marks: Vec<Element> = Vec::new();
    for n in keep {
        if marks.iter().any(|m| near(m.bounds, n.bounds)) {
            continue;
        }
        if marks.len() >= MAX_MARKS {
            break;
        }
        marks.push(Element {
            mark: marks.len() as u32 + 1,
            role: n.role,
            name: clean(&n.name),
            bounds: n.bounds,
            enabled: n.enabled,
            secure: n.secure || n.role == Role::PasswordField,
        });
    }

    if marks.len() < MIN_USEFUL {
        return grid_for(screen);
    }
    Grounding::Marks(marks)
}

/// A grid with roughly square cells, about 120px on a 1080p screen.
pub fn grid_for(screen: Rect) -> Grounding {
    let cols = ((screen.w / 120.0).round() as u32).clamp(4, 24);
    let rows = ((screen.h / 120.0).round() as u32).clamp(3, 16);
    Grounding::Grid { cols, rows, screen }
}

/// Top to bottom in lines, left to right within a line. An element joins the
/// current line when its vertical centre sits within half the line's height.
fn reading_order(mut nodes: Vec<&RawNode>) -> Vec<&RawNode> {
    nodes.sort_by(|a, b| a.bounds.center().1.total_cmp(&b.bounds.center().1));
    let mut lines: Vec<(f32, f32, Vec<&RawNode>)> = Vec::new();
    for n in nodes {
        let (_, cy) = n.bounds.center();
        match lines.last_mut() {
            Some((ly, lh, line)) if (cy - *ly).abs() <= (lh.min(n.bounds.h)) / 2.0 => line.push(n),
            _ => lines.push((cy, n.bounds.h, vec![n])),
        }
    }
    lines
        .into_iter()
        .flat_map(|(_, _, mut line)| {
            line.sort_by(|a, b| a.bounds.x.total_cmp(&b.bounds.x));
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
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
    };

    fn node(role: Role, name: &str, x: f32, y: f32) -> RawNode {
        RawNode {
            role,
            name: name.into(),
            bounds: Rect::new(x, y, 80.0, 24.0),
            enabled: true,
            visible: true,
            secure: false,
        }
    }

    #[test]
    fn numbers_in_reading_order() {
        let g = ground(
            &[
                node(Role::Button, "B", 300.0, 10.0),
                node(Role::Button, "A", 10.0, 12.0),
                node(Role::Link, "C", 10.0, 400.0),
            ],
            SCREEN,
        );
        let Grounding::Marks(m) = g else {
            panic!("expected marks")
        };
        assert_eq!(
            m.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
        assert_eq!(m.iter().map(|e| e.mark).collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    fn hidden_tiny_offscreen_and_decorative_are_dropped() {
        let mut hidden = node(Role::Button, "hidden", 10.0, 10.0);
        hidden.visible = false;
        let mut tiny = node(Role::Button, "tiny", 10.0, 50.0);
        tiny.bounds.w = 1.0;
        let nodes = [
            hidden,
            tiny,
            node(Role::Button, "off", 5000.0, 10.0),
            node(Role::Image, "logo", 10.0, 90.0),
            node(Role::Button, "ok", 10.0, 200.0),
            node(Role::Link, "home", 10.0, 300.0),
        ];
        let Grounding::Marks(m) = ground(&nodes, SCREEN) else {
            panic!()
        };
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn duplicates_collapse() {
        let Grounding::Marks(m) = ground(
            &[
                node(Role::Button, "Save", 10.0, 10.0),
                node(Role::Button, "Save", 10.5, 10.5),
                node(Role::Link, "x", 10.0, 300.0),
            ],
            SCREEN,
        ) else {
            panic!()
        };
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn password_role_is_marked_secure() {
        let Grounding::Marks(m) = ground(
            &[
                node(Role::PasswordField, "pw", 10.0, 10.0),
                node(Role::Button, "go", 10.0, 100.0),
            ],
            SCREEN,
        ) else {
            panic!()
        };
        assert!(m[0].secure);
    }

    #[test]
    fn empty_tree_falls_back_to_grid() {
        assert!(matches!(
            ground(&[], SCREEN),
            Grounding::Grid {
                cols: 16,
                rows: 9,
                ..
            }
        ));
    }

    #[test]
    fn mark_count_is_capped() {
        let nodes: Vec<RawNode> = (0..400)
            .map(|i| {
                node(
                    Role::Button,
                    "b",
                    (i % 20) as f32 * 90.0,
                    (i / 20) as f32 * 30.0,
                )
            })
            .collect();
        let Grounding::Marks(m) = ground(&nodes, SCREEN) else {
            panic!()
        };
        assert_eq!(m.len(), MAX_MARKS);
    }

    #[test]
    fn long_names_are_trimmed_to_one_line() {
        assert_eq!(clean("a\n  b \"c\""), "a b 'c'");
        assert_eq!(clean(&"x".repeat(100)).chars().count(), 61);
    }
}
