//! A shell step that starts a GUI app (`mousepad notes.txt`, `notepad x.txt`,
//! a program inside a Mac `.app` bundle) never exits by itself, so run as a
//! shell step it would block until the 180 s timeout. Such commands start
//! detached instead, on every OS (review OS-PER-02).

/// Shell syntax that makes a command more than "one program and its
/// arguments": those always run as ordinary shell steps.
const SHELL_SYNTAX: [char; 10] = ['|', ';', '&', '>', '<', '`', '$', '(', ')', '\n'];

/// Is `cmd` one GUI program and its arguments?
pub fn is_gui_command(cmd: &str) -> bool {
    let cmd = cmd.trim();
    if cmd.is_empty() || cmd.contains(SHELL_SYNTAX) {
        return false;
    }
    platform_is_gui(cmd)
}

#[cfg(target_os = "linux")]
fn platform_is_gui(cmd: &str) -> bool {
    crate::launch::linux::is_gui_command(cmd)
}

/// A program inside an app bundle (`.../TextEdit.app/Contents/MacOS/TextEdit`).
/// `open` itself returns at once, so it needs nothing.
#[cfg(target_os = "macos")]
fn platform_is_gui(cmd: &str) -> bool {
    first_word(cmd).contains(".app/Contents/MacOS/")
}

/// The first word resolves on PATH to a Windows GUI executable.
#[cfg(target_os = "windows")]
fn platform_is_gui(cmd: &str) -> bool {
    resolve_exe(first_word(cmd))
        .and_then(|p| std::fs::read(p).ok())
        .is_some_and(|b| is_gui_pe(&b))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn first_word(cmd: &str) -> &str {
    let cmd = cmd.trim_start();
    match cmd.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(""),
        None => cmd.split_whitespace().next().unwrap_or(""),
    }
}

#[cfg(target_os = "windows")]
fn resolve_exe(word: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(word);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    let named = if word.to_ascii_lowercase().ends_with(".exe") {
        word.to_string()
    } else {
        format!("{word}.exe")
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(&named))
        .find(|c| c.is_file())
}

/// A Portable Executable whose subsystem is the Windows GUI (2), as opposed
/// to the console (3). Pure, so it is tested on every OS.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_gui_pe(b: &[u8]) -> bool {
    let u16_at = |i: usize| b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    let u32_at = |i: usize| {
        b.get(i..i + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };
    if b.get(0..2) != Some(b"MZ") {
        return false;
    }
    let Some(pe) = u32_at(0x3C).map(|v| v as usize) else {
        return false;
    };
    // "PE\0\0", the 20-byte COFF header, then the optional header, whose
    // subsystem field sits at offset 68 for both PE32 and PE32+.
    b.get(pe..pe + 4) == Some(b"PE\0\0") && u16_at(pe + 24 + 68) == Some(2)
}

/// Start `cmd` without waiting for it.
pub fn spawn_detached(cmd: &str) -> anyhow::Result<()> {
    use std::process::{Command, Stdio};
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .unwrap_or_else(|| ".".into());
    #[cfg(target_os = "windows")]
    let mut c = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut c = Command::new("cmd");
        c.raw_arg(format!("/C start \"\" {cmd}"))
            .creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(target_os = "linux")]
    let mut c = {
        let mut c = Command::new("setsid");
        c.args(["-f", "sh", "-c", cmd]);
        c
    };
    #[cfg(target_os = "macos")]
    let mut c = {
        use std::os::unix::process::CommandExt;
        let mut c = Command::new("sh");
        c.args(["-c", cmd]).process_group(0);
        c
    };
    let mut child = c
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // Reap it whenever it ends, so it never lingers as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pe(subsystem: u16) -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        b[0..2].copy_from_slice(b"MZ");
        b[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x80 + 24 + 68..0x80 + 24 + 70].copy_from_slice(&subsystem.to_le_bytes());
        b
    }

    #[test]
    fn reads_the_windows_subsystem() {
        assert!(is_gui_pe(&pe(2)));
        assert!(!is_gui_pe(&pe(3)), "a console program waits normally");
        assert!(!is_gui_pe(b"MZ"));
        assert!(!is_gui_pe(b"#!/bin/sh\n"));
    }

    #[test]
    fn shell_syntax_is_never_a_gui_launch() {
        for cmd in [
            "",
            "notepad a.txt | more",
            "x && y",
            "app > out",
            "a; b",
            "$(evil)",
        ] {
            assert!(!is_gui_command(cmd), "{cmd}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bundle_programs_are_gui_on_a_mac() {
        assert!(is_gui_command(
            "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit notes.txt"
        ));
        assert!(!is_gui_command("ls -la"));
    }
}
