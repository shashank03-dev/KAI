//! What this machine lets Kai do, checked live so onboarding can show a status
//! that turns green by itself once something is granted.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Granted,
    Missing,
    Limited,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub status: Status,
    /// Why Kai needs it, in plain words.
    pub why: &'static str,
    /// What to do if it is not granted.
    pub fix: String,
}

pub async fn check_all() -> Vec<Check> {
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut checks = vec![screen().await, accessibility().await, input()];
    #[cfg(target_os = "linux")]
    if wayland() {
        checks.push(stop_shortcut_on_wayland());
    }
    checks
}

/// A Wayland desktop session (input and global shortcuts reach only XWayland).
#[cfg(not(target_os = "macos"))]
pub(crate) fn wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland"))
}

/// The global stop shortcut is an X11 key grab: on Wayland it only fires
/// while an XWayland window has focus. The corner gesture still works.
#[cfg(target_os = "linux")]
fn stop_shortcut_on_wayland() -> Check {
    Check {
        name: "Stop shortcut",
        status: Status::Limited,
        why: "The stop shortcut halts Kai from any app; the top-left corner gesture always works.",
        fix: "On Wayland the shortcut only works while an X11 app has focus: push the pointer into the top-left corner to stop Kai, or pick an \"Xorg\"/\"X11\" session at the login screen.".into(),
    }
}

pub fn all_granted(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.status != Status::Missing)
}

async fn screen() -> Check {
    let ok = tokio::task::spawn_blocking(|| {
        xcap::Monitor::all().map(|m| !m.is_empty()).unwrap_or(false) && screen_allowed()
    })
    .await
    .unwrap_or(false);
    Check {
        name: "Screen capture",
        status: if ok { Status::Granted } else { Status::Missing },
        why: "Kai looks at your screen to decide what to do next. Frames go only to your model endpoint.",
        fix: if cfg!(target_os = "linux") {
            "Allow the screen-sharing prompt from your desktop (Wayland asks once per session)."
                .into()
        } else if cfg!(target_os = "macos") {
            format!(
                "Turn Kai on under System Settings › Privacy & Security › Screen & System Audio Recording, then reopen Kai. {MAC_TERMINAL}"
            )
        } else {
            "Make sure a display is attached and Kai is not running in a locked session.".into()
        },
    }
}

/// On a Mac, run from a terminal, the permission belongs to the terminal app.
const MAC_TERMINAL: &str = "(Running `kai` from a terminal? Then allow the terminal app instead.)";

/// Whether macOS lets this process record the screen. Without it a capture
/// still succeeds but shows only the wallpaper and Kai's own windows.
#[cfg(target_os = "macos")]
fn screen_allowed() -> bool {
    objc2_core_graphics::CGPreflightScreenCaptureAccess()
}

#[cfg(not(target_os = "macos"))]
fn screen_allowed() -> bool {
    true
}

/// Whether macOS lets this process read other apps' accessibility trees and
/// send mouse and keyboard events (one permission covers both).
#[cfg(target_os = "macos")]
fn mac_trusted() -> bool {
    // SAFETY: no arguments, no preconditions.
    unsafe { objc2_application_services::AXIsProcessTrusted() }
}

#[cfg(target_os = "linux")]
async fn accessibility() -> Check {
    let on = atspi::connection::read_session_accessibility()
        .await
        .unwrap_or(false);
    Check {
        name: "Accessibility",
        status: if on { Status::Granted } else { Status::Missing },
        why: "Kai reads button and field names so it clicks the right thing instead of guessing pixels.",
        fix: "Turn on assistive technologies: `gsettings set org.gnome.desktop.interface toolkit-accessibility true`, then reopen your apps. Kai can do this for you.".into(),
    }
}

#[cfg(target_os = "macos")]
async fn accessibility() -> Check {
    Check {
        name: "Accessibility",
        status: if mac_trusted() {
            Status::Granted
        } else {
            Status::Missing
        },
        why: "Kai reads button and field names so it clicks the right thing, and moves the pointer and types for you.",
        fix: format!(
            "Turn Kai on under System Settings › Privacy & Security › Accessibility. {MAC_TERMINAL}"
        ),
    }
}

#[cfg(target_os = "windows")]
async fn accessibility() -> Check {
    Check {
        name: "Accessibility",
        status: Status::Granted,
        why: "Kai reads button and field names so it clicks the right thing instead of guessing pixels.",
        fix: "Windows UI Automation needs no permission. To control apps run as administrator, run Kai as administrator too.".into(),
    }
}

/// Other systems: no accessibility reader, so Kai works from the pixels.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
async fn accessibility() -> Check {
    Check {
        name: "Accessibility",
        status: Status::Limited,
        why: "Kai reads button and field names so it clicks the right thing instead of guessing pixels.",
        fix: "Kai has no accessibility reader for this system yet; it clicks by grid instead."
            .into(),
    }
}

/// On a Mac, input rides on the Accessibility permission checked above.
#[cfg(target_os = "macos")]
fn input() -> Check {
    Check {
        name: "Mouse and keyboard",
        status: if mac_trusted() {
            Status::Granted
        } else {
            Status::Missing
        },
        why: "Kai moves the pointer and types for you, one visible step at a time.",
        fix: "Granted together with Accessibility.".into(),
    }
}

#[cfg(not(target_os = "macos"))]
fn input() -> Check {
    let wayland = wayland();
    Check {
        name: "Mouse and keyboard",
        status: if wayland {
            Status::Limited
        } else {
            Status::Granted
        },
        why: "Kai moves the pointer and types for you, one visible step at a time.",
        fix: if wayland {
            "On Wayland, Kai can only reach XWayland apps. For full control, pick an \"Xorg\"/\"X11\" session at the login screen.".into()
        } else {
            "Nothing to do.".into()
        },
    }
}

/// Turn on the desktop's accessibility bus for apps started from now on.
#[cfg(target_os = "linux")]
pub async fn enable_accessibility() -> anyhow::Result<()> {
    atspi::connection::set_session_accessibility(true).await?;
    let _ = tokio::process::Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.interface",
            "toolkit-accessibility",
            "true",
        ])
        .status()
        .await;
    Ok(())
}

/// Ask macOS for Accessibility and Screen Recording: each shows the system
/// prompt that opens the right System Settings pane (once per app; after a
/// refusal the person turns it on there).
#[cfg(target_os = "macos")]
pub async fn enable_accessibility() -> anyhow::Result<()> {
    use objc2_application_services::{AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt};
    use objc2_core_foundation::{CFBoolean, CFDictionary};
    tokio::task::spawn_blocking(|| {
        // SAFETY: the key is the framework's own constant; the dictionary
        // maps a CFString to a CFBoolean as the API documents.
        unsafe {
            let opts =
                CFDictionary::from_slices(&[kAXTrustedCheckOptionPrompt], &[CFBoolean::new(true)]);
            AXIsProcessTrustedWithOptions(Some(opts.as_opaque()));
        }
        if !objc2_core_graphics::CGPreflightScreenCaptureAccess() {
            objc2_core_graphics::CGRequestScreenCaptureAccess();
        }
    })
    .await?;
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub async fn enable_accessibility() -> anyhow::Result<()> {
    Ok(())
}
