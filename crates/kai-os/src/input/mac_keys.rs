//! macOS: the Windows/Linux shortcut a model sends out of habit, pressed as
//! the Mac one. Models trained mostly on Linux and Windows screens send
//! `ctrl+c` or `alt+tab` despite the prompt's Mac rule; on a Mac those do
//! nothing (or something else), the screen does not change, and the model
//! retries. In a terminal `ctrl` is meant (`ctrl+c` interrupts), so only
//! the window switcher and close are mapped there.

/// Letters and keys whose `ctrl` shortcut is the `cmd` one on a Mac: copy,
/// paste, cut, undo, select all, save, find, new/close tab and window, open,
/// print, reload, address bar, quit, bold/italic/underline, zoom.
const CTRL_TO_CMD: &[&str] = &[
    "a", "b", "c", "f", "i", "l", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "z", "=",
    "+", "-", "0",
];

/// Bundle ids of terminal apps, where `ctrl` shortcuts mean `ctrl`.
const TERMINALS: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "dev.warp.Warp-Stable",
    "net.kovidgoyal.kitty",
    "org.alacritty",
    "io.alacritty",
    "com.github.wez.wezterm",
    "co.zeit.hyper",
    "com.mitchellh.ghostty",
];

/// Whether the app with this bundle id is a terminal.
pub fn is_terminal(bundle_id: &str) -> bool {
    TERMINALS.contains(&bundle_id)
}

/// The Mac form of `keys` (canonical names, modifiers first), or `None`
/// when they are already right for a Mac or have no Mac equivalent.
pub fn to_mac(keys: &[String], in_terminal: bool) -> Option<Vec<String>> {
    let (mods, rest): (Vec<&str>, Vec<&str>) = keys
        .iter()
        .map(String::as_str)
        .partition(|k| matches!(*k, "ctrl" | "alt" | "shift" | "cmd"));
    let [key] = rest[..] else {
        return None;
    };
    let shift = mods.contains(&"shift");
    let only = |m: &str| mods.iter().all(|k| *k == m || *k == "shift") && mods.contains(&m);
    let mapped: Vec<&str> = match key {
        // The window switcher, and closing a window (alt+f4 quits the app on
        // Windows; closing the window is the smaller step on a Mac).
        "tab" if only("alt") => with_shift(&["cmd"], shift, "tab"),
        "f4" if only("alt") && !shift => vec!["cmd", "w"],
        _ if in_terminal || !only("ctrl") => return None,
        // Redo: ctrl+y on Windows, cmd+shift+z on a Mac.
        "y" if !shift => vec!["cmd", "shift", "z"],
        k if CTRL_TO_CMD.contains(&k) => with_shift(&["cmd"], shift, k),
        _ => return None,
    };
    Some(mapped.into_iter().map(str::to_string).collect())
}

fn with_shift<'a>(head: &[&'a str], shift: bool, key: &'a str) -> Vec<&'a str> {
    let mut v = head.to_vec();
    if shift {
        v.push("shift");
    }
    v.push(key);
    v
}

/// Modifiers held around a click: `ctrl`+click is a right-click on a Mac,
/// so the multi-select a model means by it is `cmd`+click.
pub fn held_to_mac(mods: &[String]) -> Vec<String> {
    let has_cmd = mods.iter().any(|m| m == "cmd");
    let mut out: Vec<String> = Vec::with_capacity(mods.len());
    for m in mods {
        let m = if m == "ctrl" && !has_cmd { "cmd" } else { m };
        if !out.iter().any(|o| o == m) {
            out.push(m.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(s: &str) -> Vec<String> {
        s.split('+').map(str::to_string).collect()
    }

    fn mac(s: &str, terminal: bool) -> Option<String> {
        to_mac(&k(s), terminal).map(|v| v.join("+"))
    }

    #[test]
    fn editing_and_app_shortcuts_use_cmd() {
        for (from, to) in [
            ("ctrl+c", "cmd+c"),
            ("ctrl+v", "cmd+v"),
            ("ctrl+a", "cmd+a"),
            ("ctrl+s", "cmd+s"),
            ("ctrl+l", "cmd+l"),
            ("ctrl+shift+t", "cmd+shift+t"),
            ("ctrl+shift+z", "cmd+shift+z"),
            ("ctrl+y", "cmd+shift+z"),
            ("ctrl+=", "cmd+="),
            ("alt+tab", "cmd+tab"),
            ("alt+shift+tab", "cmd+shift+tab"),
            ("alt+f4", "cmd+w"),
        ] {
            assert_eq!(mac(from, false).as_deref(), Some(to), "{from}");
        }
    }

    #[test]
    fn mac_shortcuts_and_keys_without_a_mac_form_stay() {
        for keys in [
            "cmd+c",
            "ctrl+cmd+f",
            "enter",
            "ctrl+tab",
            "ctrl+left",
            "ctrl+space",
            "ctrl+alt+t",
            "ctrl+e",
            "ctrl+shift+y",
            "shift+tab",
            "alt+f4+x",
            "a",
        ] {
            assert_eq!(mac(keys, false), None, "{keys}");
        }
    }

    #[test]
    fn a_terminal_keeps_ctrl() {
        // ctrl+c interrupts, ctrl+r searches history, ctrl+l clears.
        for keys in ["ctrl+c", "ctrl+r", "ctrl+l", "ctrl+z", "ctrl+y"] {
            assert_eq!(mac(keys, true), None, "{keys}");
        }
        assert_eq!(mac("alt+tab", true).as_deref(), Some("cmd+tab"));
        assert!(is_terminal("com.apple.Terminal") && is_terminal("com.googlecode.iterm2"));
        assert!(!is_terminal("com.apple.TextEdit"));
    }

    #[test]
    fn ctrl_click_becomes_cmd_click() {
        assert_eq!(held_to_mac(&k("ctrl")), k("cmd"));
        assert_eq!(held_to_mac(&k("ctrl+shift")), k("cmd+shift"));
        assert_eq!(held_to_mac(&k("ctrl+cmd")), k("ctrl+cmd"));
        assert_eq!(held_to_mac(&k("shift")), k("shift"));
    }
}
