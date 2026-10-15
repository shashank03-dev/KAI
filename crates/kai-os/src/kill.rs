//! The kill switch. A global hotkey and pushing the pointer into the top-left
//! corner both set one flag; the engine checks it before every step, so Kai
//! stops within one step. The interrupt button in the UI sets the same flag.

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
    while running.load(Ordering::SeqCst) {
        if let Ok((x, y)) = input.cursor().await
            && in_corner(x, y)
        {
            kill.trip("pointer in top-left corner");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub fn in_corner(x: i32, y: i32) -> bool {
    x <= CORNER && y <= CORNER
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
        assert!(in_corner(0, 0));
        assert!(in_corner(3, 2));
        assert!(!in_corner(4, 0));
        assert!(!in_corner(500, 500));
    }

    #[test]
    fn default_hotkeys_parse() {
        assert!(DEFAULT_HOTKEY.parse::<HotKey>().is_ok());
        assert!(FALLBACK_HOTKEY.parse::<HotKey>().is_ok());
    }
}
