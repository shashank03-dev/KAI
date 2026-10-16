//! The kill switch. A global hotkey and pushing the pointer into the top-left
//! corner of the primary display both set one flag; the engine checks it
//! before every step, so Kai stops within one step. The interrupt button in
//! the UI sets the same flag.

use crate::input::Input;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Default stop shortcut.
pub const DEFAULT_HOTKEY: &str = "ctrl+alt+Escape";
/// Used when the default is already taken (KDE binds Ctrl+Alt+Esc to xkill).
pub const FALLBACK_HOTKEY: &str = "ctrl+alt+End";
/// Pixels from the top-left corner that count as "in the corner".
pub const CORNER: i32 = 3;
/// Whether a global hotkey fires without a UI event loop. macOS delivers
/// hotkey events only through the main thread's run loop, which the app
/// runs and the `kai` command line does not; there the corner is the stop.
pub const HOTKEY_WITHOUT_EVENT_LOOP: bool = !cfg!(target_os = "macos");

/// Shared stop flag. Clone freely.
#[derive(Clone, Default)]
pub struct KillSwitch {
    flag: Arc<AtomicBool>,
}

impl KillSwitch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trip(&self, why: &str) {
        if !self.flag.swap(true, Ordering::SeqCst) {
            log::warn!("kill switch tripped: {why}");
        }
    }

    /// Clear before a new run.
    pub fn reset(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }

    pub fn is_tripped(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

impl kai_core::Interrupt for KillSwitch {
    fn stopped(&self) -> bool {
        self.is_tripped()
    }
}

/// Keeps the hotkey registered while alive. On Windows, create this on a
/// thread that runs an event loop (the GPUI main thread).
pub struct Hotkey {
    _manager: GlobalHotKeyManager,
    pub shortcut: String,
}

/// Register `shortcut` (or the fallback if it is taken) to trip `kill`.
pub fn install_hotkey(kill: &KillSwitch, shortcut: &str) -> anyhow::Result<Hotkey> {
    #[cfg(target_os = "linux")]
    if crate::permissions::wayland() {
        log::warn!(
            "on Wayland the stop shortcut only fires while an X11 (XWayland) window has focus; \
             push the pointer into the top-left corner to stop Kai"
        );
    }
    let manager = GlobalHotKeyManager::new()?;
    let mut used = shortcut.to_string();
    let key: HotKey = shortcut.parse()?;
    let key = match manager.register(key) {
        Ok(()) => key,
        Err(e) => {
            log::warn!("{shortcut} unavailable ({e}); using {FALLBACK_HOTKEY}");
            let fb: HotKey = FALLBACK_HOTKEY.parse()?;
            manager.register(fb)?;
            used = FALLBACK_HOTKEY.to_string();
            fb
        }
    };
    let id = key.id();
    let kill = kill.clone();
    std::thread::Builder::new()
        .name("kai-hotkey".into())
        .spawn(move || {
            while let Ok(ev) = GlobalHotKeyEvent::receiver().recv() {
                if ev.id == id && ev.state == HotKeyState::Pressed {
                    kill.trip("hotkey");
                }
            }
        })?;
    Ok(Hotkey {
        _manager: manager,
        shortcut: used,
    })
}

/// Watch the pointer while a run is active; the corner gesture trips the switch.
pub async fn watch_corner(kill: KillSwitch, input: Input, running: Arc<AtomicBool>) {
    let origin = tokio::task::spawn_blocking(primary_origin)
        .await
        .unwrap_or_default();
    let mut corner = Corner::default();
    while running.load(Ordering::SeqCst) {
        let at = input.cursor().await.ok();
        let moves = input.pointer_moves();
        let kai_moved = moves != corner.moves || input.pointer_busy();
        corner.moves = moves;
        match corner.step(at.map(|(x, y)| in_corner(x, y, origin)), kai_moved) {
            Some(Seen::Stop) => kill.trip("pointer in top-left corner"),
            Some(Seen::Blind) => {
                log::warn!("the pointer position cannot be read; the corner stop is off")
            }
            None => {}
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The primary display's top-left point (not always 0,0 on X11).
fn primary_origin() -> (i32, i32) {
    let Ok(all) = xcap::Monitor::all() else {
        return (0, 0);
    };
    let primary = all
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or(all.first());
    primary
        .and_then(|m| Some((m.x().ok()?, m.y().ok()?)))
        .unwrap_or_default()
}

pub fn in_corner(x: i32, y: i32, origin: (i32, i32)) -> bool {
    (origin.0..=origin.0 + CORNER).contains(&x) && (origin.1..=origin.1 + CORNER).contains(&y)
}

/// Polls in the corner, without Kai moving the pointer, that make a stop
/// (two polls ≈ 150 ms of holding it there).
const HOLD_POLLS: u32 = 2;
/// Failed reads in a row before the corner stop says it is blind.
const BLIND_POLLS: u32 = 20;

/// What the corner watcher concluded from one poll.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Stop,
    /// The pointer cannot be read (said once).
    Blind,
}

/// The corner gesture as a state machine over polls. Only the person's
/// push counts: the pointer must have been seen outside the corner first
/// (a pointer parked there when the run starts does not stop it), and an
/// entry Kai made itself (a click or drag into the corner) does not count
/// until the pointer has left again.
#[derive(Default)]
struct Corner {
    armed: bool,
    held: u32,
    failures: u32,
    /// [`Input::pointer_moves`] at the last poll.
    moves: u64,
}

impl Corner {
    /// One poll: `inside` is `None` when the pointer could not be read;
    /// `kai_moved` says Kai moved the pointer since the previous poll.
    fn step(&mut self, inside: Option<bool>, kai_moved: bool) -> Option<Seen> {
        let Some(inside) = inside else {
            self.failures += 1;
            return (self.failures == BLIND_POLLS).then_some(Seen::Blind);
        };
        self.failures = 0;
        if !inside {
            self.armed = true;
            self.held = 0;
            return None;
        }
        if kai_moved {
            self.armed = false;
        }
        if !self.armed {
            self.held = 0;
            return None;
        }
        self.held += 1;
        (self.held >= HOLD_POLLS).then_some(Seen::Stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kai_core::Interrupt;

    #[test]
    fn trip_and_reset() {
        let k = KillSwitch::new();
        let other = k.clone();
        assert!(!k.stopped());
        other.trip("test");
        assert!(k.stopped());
        k.reset();
        assert!(!other.stopped());
    }

    #[test]
    fn corner_detection() {
        assert!(in_corner(0, 0, (0, 0)));
        assert!(in_corner(3, 2, (0, 0)));
        assert!(!in_corner(4, 0, (0, 0)));
        assert!(!in_corner(500, 500, (0, 0)));
        // A primary display right of another one starts at x = 1920.
        assert!(in_corner(1921, 1, (1920, 0)));
        assert!(!in_corner(0, 0, (1920, 0)));
        assert!(in_corner(-1919, 2, (-1920, 0)));
    }

    fn run(corner: &mut Corner, polls: &[(Option<bool>, bool)]) -> Vec<Option<Seen>> {
        polls.iter().map(|&(i, k)| corner.step(i, k)).collect()
    }

    #[test]
    fn a_push_into_the_corner_stops_after_a_short_hold() {
        let mut c = Corner::default();
        let seen = run(&mut c, &[(Some(false), false), (Some(true), false)]);
        assert_eq!(seen, [None, None], "one poll is not a hold yet");
        assert_eq!(c.step(Some(true), false), Some(Seen::Stop));
    }

    #[test]
    fn a_parked_pointer_needs_to_leave_and_come_back() {
        let mut c = Corner::default();
        let seen = run(&mut c, &[(Some(true), false); 5]);
        assert!(seen.iter().all(Option::is_none));
        run(&mut c, &[(Some(false), false), (Some(true), false)]);
        assert_eq!(c.step(Some(true), false), Some(Seen::Stop));
    }

    #[test]
    fn kai_moving_into_the_corner_is_not_a_stop() {
        let mut c = Corner::default();
        c.step(Some(false), false);
        let seen = run(
            &mut c,
            &[(Some(true), true), (Some(true), false), (Some(true), false)],
        );
        assert!(seen.iter().all(Option::is_none), "{seen:?}");
        // The person moves it out and back in: that counts again.
        run(&mut c, &[(Some(false), false), (Some(true), false)]);
        assert_eq!(c.step(Some(true), false), Some(Seen::Stop));
    }

    #[test]
    fn an_unreadable_pointer_is_reported_once() {
        let mut c = Corner::default();
        let seen = run(&mut c, &[(None, false); 30]);
        assert_eq!(seen.iter().filter(|s| s.is_some()).count(), 1);
        assert_eq!(seen[BLIND_POLLS as usize - 1], Some(Seen::Blind));
    }

    #[test]
    fn default_hotkeys_parse() {
        assert!(DEFAULT_HOTKEY.parse::<HotKey>().is_ok());
        assert!(FALLBACK_HOTKEY.parse::<HotKey>().is_ok());
    }
}
