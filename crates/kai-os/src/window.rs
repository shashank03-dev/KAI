//! Window-scoped frames (`KAI_WINDOW_SCOPE=1`, off by default): the model
//! sees the focused app, not the whole display. The region is the focused
//! window plus every other visible window of the same process (its menus,
//! popups and dialogs are separate top-level windows on X11 and Windows),
//! cut out of the normal display capture. Other apps never reach the model,
//! and a small window costs fewer image tokens. Design and limits:
//! `docs/BACKGROUND.md`.

use kai_core::Rect;

/// One top-level window as the OS lists it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Win {
    pub pid: u32,
    pub rect: Rect,
    pub focused: bool,
    pub minimized: bool,
}

/// The region to show the model: the focused window and its process's other
/// visible windows, clamped to `display`. `None` (use the whole display)
/// when no window has focus, the focused window is `own_pid` (Kai itself),
/// or nothing of it is on this display.
pub fn region(wins: &[Win], display: Rect, own_pid: u32) -> Option<Rect> {
    let f = wins.iter().find(|w| w.focused && !w.minimized)?;
    if f.pid == own_pid {
        return None;
    }
    let mut r: Option<Rect> = None;
    for w in wins
        .iter()
        .filter(|w| w.pid == f.pid && !w.minimized && w.rect.w >= 1.0 && w.rect.h >= 1.0)
    {
        let Some(c) = clamp(w.rect, display) else {
            continue;
        };
        r = Some(match r {
            None => c,
            Some(u) => union(u, c),
        });
    }
    // Too small to be a real window (a 1x1 helper): show everything.
    r.filter(|r| r.w >= 32.0 && r.h >= 32.0)
}

/// The windows on this machine (X11, Windows). Wayland lists none.
pub fn list() -> Vec<Win> {
    let Ok(all) = xcap::Window::all() else {
        return Vec::new();
    };
    all.iter()
        .filter_map(|w| {
            Some(Win {
                pid: w.pid().ok()?,
                rect: Rect::new(
                    w.x().ok()? as f32,
                    w.y().ok()? as f32,
                    w.width().ok()? as f32,
                    w.height().ok()? as f32,
                ),
                focused: w.is_focused().unwrap_or(false),
                minimized: w.is_minimized().unwrap_or(false),
            })
        })
        .collect()
}

/// The region for the live desktop, or `None` for the whole display.
pub fn focused_region(display: Rect) -> Option<Rect> {
    region(&list(), display, std::process::id())
}

fn clamp(r: Rect, d: Rect) -> Option<Rect> {
    let x0 = r.x.max(d.x);
    let y0 = r.y.max(d.y);
    let x1 = (r.x + r.w).min(d.x + d.w);
    let y1 = (r.y + r.h).min(d.y + d.h);
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

fn union(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
    };

    fn win(pid: u32, x: f32, y: f32, w: f32, h: f32, focused: bool) -> Win {
        Win {
            pid,
            rect: Rect::new(x, y, w, h),
            focused,
            minimized: false,
        }
    }

    #[test]
    fn the_focused_app_and_its_popups_but_not_other_apps() {
        let wins = [
            win(10, 100.0, 100.0, 800.0, 600.0, true),
            // Its menu, partly outside the main window.
            win(10, 850.0, 150.0, 200.0, 300.0, false),
            // Another app beside it.
            win(20, 1000.0, 0.0, 900.0, 1000.0, false),
        ];
        assert_eq!(
            region(&wins, DISPLAY, 1),
            Some(Rect::new(100.0, 100.0, 950.0, 600.0))
        );
    }

    #[test]
    fn whole_display_when_focus_is_unknown_or_kai_itself() {
        let other = win(20, 0.0, 0.0, 500.0, 500.0, false);
        assert_eq!(region(&[other], DISPLAY, 1), None);
        let kai = win(1, 0.0, 0.0, 500.0, 500.0, true);
        assert_eq!(region(&[kai, other], DISPLAY, 1), None);
        assert_eq!(region(&[], DISPLAY, 1), None);
    }

    #[test]
    fn clamped_to_the_display_and_never_a_sliver() {
        let off = win(10, 1800.0, 900.0, 400.0, 400.0, true);
        assert_eq!(
            region(&[off], DISPLAY, 1),
            Some(Rect::new(1800.0, 900.0, 120.0, 180.0))
        );
        let gone = win(10, 3000.0, 0.0, 400.0, 400.0, true);
        assert_eq!(region(&[gone], DISPLAY, 1), None);
        let sliver = win(10, 0.0, 0.0, 10.0, 400.0, true);
        assert_eq!(region(&[sliver], DISPLAY, 1), None);
        let mut min = win(10, 0.0, 0.0, 400.0, 400.0, true);
        min.minimized = true;
        assert_eq!(region(&[min], DISPLAY, 1), None);
    }
}
