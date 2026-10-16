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

/// The focused window of another app (Kai's own does not count), in global
/// coordinates, for picking the display to capture.
pub fn focused_rect() -> Option<Rect> {
    #[cfg(target_os = "macos")]
    {
        // xcap re-reads the whole window list per window property; the
        // accessibility API answers this in three calls.
        crate::a11y::focused_window_rect()
    }
    #[cfg(not(target_os = "macos"))]
    {
        focused_of(&list(), std::process::id())
    }
}

/// The focused, not minimised window's rect, unless it is `own_pid`'s.
pub fn focused_of(wins: &[Win], own_pid: u32) -> Option<Rect> {
    wins.iter()
        .find(|w| w.focused && !w.minimized)
        .filter(|w| w.pid != own_pid)
        .map(|w| w.rect)
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

/// What the window list looks like: (id, title, focused) per window.
pub type Shot = Vec<(u32, String, bool)>;

/// The window list now; `None` when it cannot be read (Wayland lists no
/// windows, so an empty list there says nothing). An empty desktop on X11
/// or Windows is `Some(empty)`.
pub fn shot() -> Option<Shot> {
    #[cfg(target_os = "linux")]
    std::env::var_os("DISPLAY")?;
    let all = xcap::Window::all().ok()?;
    let shot = all
        .iter()
        .filter_map(|w| {
            Some((
                w.id().ok()?,
                w.title().unwrap_or_default(),
                w.is_focused().unwrap_or(false),
            ))
        })
        .collect();
    Some(shot)
}

/// Should a launch wait on? Only when the windows can be listed at all
/// and nothing opened, closed, got focus or changed title since.
pub fn unchanged(before: &Option<Shot>, now: &Option<Shot>) -> bool {
    before.is_some() && before == now
}

/// After starting an app or opening a link: wait (at most `max`) until the
/// window list changes, so the next look sees the app and not the desktop
/// it is still starting on (seen live: a cold Firefox took longer than the
/// screen settle, the model saw a black screen and opened it three more
/// times). `false` only when the list was read and nothing changed; where
/// windows cannot be listed (Wayland) a launch cannot be verified and
/// counts as seen.
pub fn wait_for_change(before: &Option<Shot>, max: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + max;
    loop {
        if !unchanged(before, &shot()) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
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
    fn the_focused_window_is_another_apps() {
        let mut wins = [
            win(20, 2000.0, 0.0, 800.0, 600.0, false),
            win(10, -1500.0, 100.0, 800.0, 600.0, true),
        ];
        assert_eq!(focused_of(&wins, 1), Some(wins[1].rect));
        assert_eq!(focused_of(&wins, 10), None, "Kai itself in front");
        wins[1].minimized = true;
        assert_eq!(focused_of(&wins, 1), None);
    }

    #[test]
    fn a_launch_waits_only_while_nothing_changed_and_windows_are_listed() {
        let term = |focused| (1, "Terminal".to_string(), focused);
        let before: Option<Shot> = Some(vec![term(true)]);
        assert!(unchanged(&before, &before.clone()));
        let opened = Some(vec![term(false), (2, "Firefox".into(), true)]);
        assert!(!unchanged(&before, &opened));
        let tab = |title: &str| Some(vec![(1, title.to_string(), true)]);
        assert!(!unchanged(&tab("New Tab"), &tab("Opening hours")));
        // An empty X11 desktop waits for the first window (seen live: no
        // wait there, and the model saw a black screen).
        assert!(unchanged(&Some(Vec::new()), &Some(Vec::new())));
        assert!(!unchanged(&Some(Vec::new()), &opened));
        // No window list at all (Wayland): never wait.
        assert!(!unchanged(&None, &None));
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
