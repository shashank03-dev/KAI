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
    vec![screen().await, accessibility().await, input()]
}

pub fn all_granted(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.status != Status::Missing)
}

async fn screen() -> Check {
    let ok = tokio::task::spawn_blocking(|| {
        xcap::Monitor::all().map(|m| !m.is_empty()).unwrap_or(false)
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
        } else {
            "Make sure a display is attached and Kai is not running in a locked session.".into()
        },
    }
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

#[cfg(not(target_os = "linux"))]
async fn accessibility() -> Check {
    Check {
        name: "Accessibility",
        status: Status::Granted,
        why: "Kai reads button and field names so it clicks the right thing instead of guessing pixels.",
        fix: "Windows UI Automation needs no permission. To control apps run as administrator, run Kai as administrator too.".into(),
    }
}

fn input() -> Check {
    let wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false);
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

#[cfg(not(target_os = "linux"))]
pub async fn enable_accessibility() -> anyhow::Result<()> {
    Ok(())
}
