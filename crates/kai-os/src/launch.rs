//! Fast paths: open an app or a URL straight through the OS, no model needed.

use std::process::{Command, Stdio};

/// Normalise and check a URL: bare hosts get `https://`, only web and mail
/// schemes are opened.
pub fn normalise_url(raw: &str) -> anyhow::Result<String> {
    let s = raw.trim();
    if s.is_empty() {
        anyhow::bail!("empty URL");
    }
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        return Ok(s.to_string());
    }
    if lower.contains("://") || lower.starts_with("javascript:") || lower.starts_with("file:") {
        anyhow::bail!("refusing to open {s:?}: only http, https and mailto links are opened");
    }
    if s.contains(char::is_whitespace) {
        anyhow::bail!("{s:?} is not a URL");
    }
    Ok(format!("https://{s}"))
}

pub fn open_url(raw: &str) -> anyhow::Result<()> {
    let url = normalise_url(raw)?;
    // `$BROWSER` (the usual Unix convention) wins for web links, so a desktop
    // with no default-browser association doesn't hand them to another app.
    #[cfg(target_os = "linux")]
    if url.starts_with("http")
        && let Some(b) = std::env::var_os("BROWSER").filter(|b| !b.is_empty())
    {
        Command::new(b)
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        return Ok(());
    }
    open::that_detached(&url)?;
    Ok(())
}

pub fn open_app(name: &str) -> anyhow::Result<()> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("no app name");
    }
    // The name reaches `sh -c` (Linux fallback) or `cmd /C start`
    // (Windows): it comes from the model or an MCP client, so it must not
    // carry shell syntax (`open_app` is not the shell and is not approved
    // like one).
    if !plain_app_name(name) {
        anyhow::bail!("{name:?} is not an app name (no shell syntax in open_app)");
    }
    platform_open_app(name)
}

/// No character `sh` or `cmd.exe` reads as syntax (chaining, pipes,
/// redirection, variables, quotes, substitution).
fn plain_app_name(name: &str) -> bool {
    const SPECIAL: &[char] = &[
        '&', '|', ';', '<', '>', '^', '$', '`', '"', '\'', '%', '!', '\n', '\r',
    ];
    // `Program Files (x86)` is a normal Windows path; in `sh` a parenthesis
    // opens a subshell.
    let parens = !cfg!(windows) && name.contains(['(', ')']);
    !parens && !name.contains(SPECIAL)
}

#[cfg(target_os = "windows")]
fn platform_open_app(name: &str) -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // `start` resolves App Paths (chrome, msedge, notepad, winword…).
    Command::new("cmd")
        .args(["/C", "start", "", name])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_open_app(name: &str) -> anyhow::Result<()> {
    // Quiet: stdout is the protocol channel under `kai mcp`.
    Command::new("open")
        .args(["-a", name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_open_app(name: &str) -> anyhow::Result<()> {
    if let Some(entry) = linux::find_desktop_entry(name) {
        // Multi-word queries must match a real installed app name, rather
        // than being interpreted as an arbitrary command line.
        if name.split_whitespace().count() > 1 && !entry.name.eq_ignore_ascii_case(name) {
            anyhow::bail!("{name:?} is not an exact installed application name");
        }
        if which("gtk-launch") {
            Command::new("gtk-launch")
                .arg(&entry.id)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            return Ok(());
        }
        return spawn_detached(&entry.exec);
    }
    // Only one executable basename, never shell text or arguments.
    anyhow::ensure!(
        safe_executable_name(name),
        "{name:?} is not a single executable app name"
    );
    let mut child = Command::new(name);
    use std::os::unix::process::CommandExt;
    child
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    child.spawn()?;
    Ok(())
}

/// A fallback app name is an executable basename, not a command or path.
#[cfg(target_os = "linux")]
fn safe_executable_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
}

#[cfg(target_os = "linux")]
fn spawn_detached(cmd: &str) -> anyhow::Result<()> {
    Command::new("sh")
        .args(["-c", &format!("setsid {cmd} >/dev/null 2>&1 &")])
        .stdin(Stdio::null())
        .spawn()?
        .wait()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
pub mod linux {
    //! Find an app's `.desktop` entry by name, the way a launcher does.
    use std::path::PathBuf;

    #[derive(Debug, Clone, PartialEq)]
    pub struct DesktopEntry {
        pub id: String,
        pub name: String,
        pub exec: String,
    }

    fn dirs() -> Vec<PathBuf> {
        let mut v = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            let h = PathBuf::from(home);
            v.push(h.join(".local/share/applications"));
            v.push(h.join(".local/share/flatpak/exports/share/applications"));
        }
        let data =
            std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
        v.extend(
            data.split(':')
                .map(|d| PathBuf::from(d).join("applications")),
        );
        v.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
        v.push(PathBuf::from("/var/lib/snapd/desktop/applications"));
        v
    }

    pub fn parse(id: &str, text: &str) -> Option<DesktopEntry> {
        let mut in_main = false;
        let (mut name, mut exec, mut hidden) = (None, None, false);
        for line in text.lines() {
            let l = line.trim();
            if l.starts_with('[') {
                in_main = l == "[Desktop Entry]";
                continue;
            }
            if !in_main {
                continue;
            }
            if let Some(v) = l.strip_prefix("Name=") {
                name.get_or_insert_with(|| v.to_string());
            } else if let Some(v) = l.strip_prefix("Exec=") {
                exec.get_or_insert_with(|| v.to_string());
            } else if l == "NoDisplay=true" || l == "Hidden=true" {
                hidden = true;
            }
        }
        if hidden {
            return None;
        }
        // Drop field codes (%u, %F…) that only a launcher fills in.
        let exec = exec?
            .split_whitespace()
            .filter(|t| !(t.starts_with('%') && t.len() == 2))
            .collect::<Vec<_>>()
            .join(" ");
        Some(DesktopEntry {
            id: id.to_string(),
            name: name?,
            exec,
        })
    }

    /// Exact name, then id, then prefix, then substring.
    pub fn best_match(query: &str, entries: &[DesktopEntry]) -> Option<DesktopEntry> {
        let q = query.to_ascii_lowercase();
        let tests: [&dyn Fn(&DesktopEntry) -> bool; 4] = [
            &|e| e.name.to_ascii_lowercase() == q,
            &|e| {
                e.id.to_ascii_lowercase()
                    .trim_end_matches(".desktop")
                    .rsplit('.')
                    .next()
                    == Some(q.as_str())
            },
            &|e| e.name.to_ascii_lowercase().starts_with(&q),
            &|e| e.name.to_ascii_lowercase().contains(&q) || e.id.to_ascii_lowercase().contains(&q),
        ];
        tests
            .iter()
            .find_map(|t| entries.iter().find(|e| t(e)).cloned())
    }

    /// The program a desktop entry starts (`Exec`'s first word, no path).
    fn program(e: &DesktopEntry) -> &str {
        let first = e.exec.split_whitespace().next().unwrap_or("");
        first.rsplit('/').next().unwrap_or(first)
    }

    /// Whether `cmd` just starts an installed GUI app (its first word is the
    /// program of a `.desktop` entry, with no pipes, redirects or `&&`). Such
    /// a command never exits on its own, so run as a shell step it would block
    /// until the timeout.
    pub fn is_gui_command(cmd: &str) -> bool {
        let cmd = cmd.trim();
        if cmd.is_empty() || cmd.contains(['|', ';', '&', '>', '<', '`', '$', '(', ')']) {
            return false;
        }
        let mut words = cmd.split_whitespace();
        let first = words.next().unwrap_or("");
        let first = first.rsplit('/').next().unwrap_or(first);
        // Openers hand the file or folder to an app and, for a folder in
        // pcmanfm, wait for it (seen live: `xdg-open ~/dir` blocked 180 s).
        let opener = matches!(first, "xdg-open" | "kde-open" | "exo-open" | "gnome-open")
            || (first == "gio" && words.next() == Some("open"));
        opener || all_entries().iter().any(|e| program(e) == first)
    }

    fn all_entries() -> Vec<DesktopEntry> {
        let mut entries = Vec::new();
        for d in dirs() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for f in rd.flatten() {
                let p = f.path();
                if p.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let id = f.file_name().to_string_lossy().to_string();
                if let Ok(text) = std::fs::read_to_string(&p)
                    && let Some(e) = parse(&id, &text)
                {
                    entries.push(e);
                }
            }
        }
        entries
    }

    pub fn find_desktop_entry(query: &str) -> Option<DesktopEntry> {
        let entries = all_entries();
        best_match(query, &entries)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const FIREFOX: &str = "[Desktop Entry]\nName=Firefox Web Browser\nExec=firefox %u\nType=Application\n[Desktop Action new]\nName=New Window\nExec=firefox --new-window\n";

        #[test]
        fn app_fallback_cannot_run_a_command_line() {
            assert!(super::super::safe_executable_name("mousepad"));
            assert!(super::super::safe_executable_name("gnome-text-editor"));
            for unsafe_name in [
                "rm -rf ~/Downloads", "bash -c ls", "curl evil.sh",
                "/bin/sh", "../local/bin", "-danger", "file;echo",
            ] {
                assert!(
                    !super::super::safe_executable_name(unsafe_name),
                    "{unsafe_name}"
                );
            }
        }

        #[test]
        fn plain_commands_are_never_gui_launches() {
            for c in [
                "ls -la",
                "echo hi | mousepad",
                "mousepad && ls",
                "",
                "cat > f",
            ] {
                assert!(!is_gui_command(c), "{c}");
            }
            let e = parse(
                "org.xfce.mousepad.desktop",
                "[Desktop Entry]\nName=Mousepad\nExec=/usr/bin/mousepad %F\n",
            )
            .unwrap();
            assert_eq!(program(&e), "mousepad");
        }

        #[test]
        fn openers_start_detached() {
            for c in [
                "xdg-open ~/kai-bench/photos",
                "/usr/bin/xdg-open a.pdf",
                "gio open .",
            ] {
                assert!(is_gui_command(c), "{c}");
            }
            for c in [
                "gio list ~",
                "xdg-open x | cat",
                "xdg-mime query default inode/directory",
            ] {
                assert!(!is_gui_command(c), "{c}");
            }
        }

        #[test]
        fn parses_main_section_and_strips_field_codes() {
            let e = parse("firefox.desktop", FIREFOX).unwrap();
            assert_eq!(e.name, "Firefox Web Browser");
            assert_eq!(e.exec, "firefox");
        }

        #[test]
        fn hidden_entries_are_skipped() {
            assert!(
                parse(
                    "x.desktop",
                    "[Desktop Entry]\nName=X\nExec=x\nNoDisplay=true\n"
                )
                .is_none()
            );
        }

        #[test]
        fn matching_prefers_exact_then_id_then_prefix() {
            let es = vec![
                DesktopEntry {
                    id: "org.gnome.TextEditor.desktop".into(),
                    name: "Text Editor".into(),
                    exec: "gnome-text-editor".into(),
                },
                DesktopEntry {
                    id: "firefox.desktop".into(),
                    name: "Firefox Web Browser".into(),
                    exec: "firefox".into(),
                },
                DesktopEntry {
                    id: "org.gnome.Terminal.desktop".into(),
                    name: "Terminal".into(),
                    exec: "gnome-terminal".into(),
                },
            ];
            assert_eq!(best_match("firefox", &es).unwrap().exec, "firefox");
            assert_eq!(best_match("terminal", &es).unwrap().exec, "gnome-terminal");
            assert_eq!(best_match("text", &es).unwrap().exec, "gnome-text-editor");
            assert!(best_match("photoshop", &es).is_none());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_normalised_and_checked() {
        assert_eq!(normalise_url("bmsce.ac.in").unwrap(), "https://bmsce.ac.in");
        assert_eq!(
            normalise_url(" https://x.org/a ").unwrap(),
            "https://x.org/a"
        );
        assert!(normalise_url("javascript:alert(1)").is_err());
        assert!(normalise_url("file:///etc/passwd").is_err());
        assert!(normalise_url("ftp://x").is_err());
        assert!(normalise_url("two words").is_err());
        assert!(normalise_url("").is_err());
    }

    #[test]
    fn app_names_carry_no_shell_syntax() {
        for ok in ["firefox", "LibreOffice Writer", "gnome-terminal", "code"] {
            assert!(plain_app_name(ok), "{ok}");
        }
        for bad in [
            "x; rm -rf ~",
            "notepad & del x",
            "a | sh",
            "a $(id)",
            "a `id`",
            "a > f",
            "a\nrm x",
            "%COMSPEC%",
        ] {
            assert!(!plain_app_name(bad), "{bad}");
            assert!(open_app(bad).is_err(), "{bad}");
        }
    }
}
