//! macOS: open an app by name with `open -a`. Models trained mostly on
//! Linux and Windows screens ask for "File Explorer", "Notepad" or
//! "System Preferences"; when the name itself is not an installed app, the
//! Mac app that does the same job is opened instead.

/// Common Linux and Windows app names (lowercase) and the Mac app for each.
const MAC_APPS: &[(&[&str], &str)] = &[
    (
        &[
            "system preferences",
            "settings",
            "control panel",
            "preferences",
        ],
        "System Settings",
    ),
    (
        &[
            "file explorer",
            "explorer",
            "files",
            "file manager",
            "nautilus",
            "dolphin",
            "thunar",
        ],
        "Finder",
    ),
    (
        &[
            "notepad",
            "text editor",
            "gedit",
            "mousepad",
            "kate",
            "wordpad",
        ],
        "TextEdit",
    ),
    (
        &[
            "terminal",
            "cmd",
            "command prompt",
            "powershell",
            "gnome-terminal",
            "konsole",
            "xterm",
        ],
        "Terminal",
    ),
    (
        &["calculator", "calc", "gnome-calculator", "kcalc"],
        "Calculator",
    ),
    (
        &["task manager", "system monitor", "gnome-system-monitor"],
        "Activity Monitor",
    ),
    (
        &["chrome", "google-chrome", "google chrome"],
        "Google Chrome",
    ),
    (&["edge", "msedge", "microsoft edge"], "Microsoft Edge"),
    (&["vscode", "vs code", "code"], "Visual Studio Code"),
    (&["browser", "web browser"], "Safari"),
    (&["snipping tool", "screenshot"], "Screenshot"),
    (&["word", "winword"], "Microsoft Word"),
    (&["excel"], "Microsoft Excel"),
    (&["powerpoint"], "Microsoft PowerPoint"),
    (&["outlook"], "Microsoft Outlook"),
];

/// The Mac app that does the job of `name`, if it is a known other-OS name.
pub(crate) fn mac_equivalent(name: &str) -> Option<&'static str> {
    let lower = name.trim().to_lowercase();
    let lower = lower.strip_suffix(".exe").unwrap_or(&lower);
    MAC_APPS
        .iter()
        .find(|(names, _)| names.contains(&lower))
        .map(|(_, app)| *app)
        .filter(|app| !app.eq_ignore_ascii_case(name.trim()))
}

/// Open `name`, else its Mac equivalent; the error is the one for `name`.
#[cfg(target_os = "macos")]
pub(crate) fn open_app(name: &str) -> anyhow::Result<()> {
    let first = open_a(name);
    let Err(e) = first else {
        return Ok(());
    };
    match mac_equivalent(name) {
        Some(app) if open_a(app).is_ok() => {
            log::info!("opened {app:?} for {name:?} (the Mac app for it)");
            Ok(())
        }
        _ => Err(e),
    }
}

#[cfg(target_os = "macos")]
fn open_a(name: &str) -> anyhow::Result<()> {
    use std::process::{Command, Stdio};
    // Quiet: stdout is the protocol channel under `kai mcp`. `open -a`
    // returns once Launch Services has started the app, and fails for an
    // unknown name ("Unable to find application named …").
    let out = Command::new("open")
        .args(["-a", name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("could not open {name:?}: {}", why.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_os_names_map_to_the_mac_app() {
        assert_eq!(mac_equivalent("File Explorer"), Some("Finder"));
        assert_eq!(mac_equivalent("notepad.exe"), Some("TextEdit"));
        assert_eq!(
            mac_equivalent(" System Preferences "),
            Some("System Settings")
        );
        assert_eq!(mac_equivalent("chrome"), Some("Google Chrome"));
        assert_eq!(mac_equivalent("gnome-calculator"), Some("Calculator"));
    }

    #[test]
    fn mac_and_unknown_names_have_no_equivalent() {
        // Already the Mac name (any case): nothing else to try.
        assert_eq!(mac_equivalent("Calculator"), None);
        assert_eq!(mac_equivalent("terminal"), None);
        assert_eq!(mac_equivalent("Slack"), None);
        assert_eq!(mac_equivalent(""), None);
    }
}
