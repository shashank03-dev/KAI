//! Kai's key chords (`["ctrl","a"]`, `["enter"]`) as DevTools key events.
//! Protocol key events reach the page, not the browser window, so browser
//! shortcuts (back, forward, reload) become navigation commands instead.

use serde_json::{Value, json};

/// What one chord does in the browser lane.
#[derive(Debug, Clone, PartialEq)]
pub enum Chord {
    /// `keyDown`/`keyUp` pairs for the page; `params` are the `keyDown`
    /// params (the `keyUp` drops `text` and `commands`).
    Keys(Vec<Value>),
    Back,
    Forward,
    Reload,
}

const ALT: i64 = 1;
const CTRL: i64 = 2;
const META: i64 = 4;
const SHIFT: i64 = 8;

/// `(key, code, windowsVirtualKeyCode, text)` for named keys.
fn named(k: &str) -> Option<(&'static str, &'static str, i64, Option<&'static str>)> {
    Some(match k {
        "enter" => ("Enter", "Enter", 13, Some("\r")),
        "tab" => ("Tab", "Tab", 9, None),
        "esc" => ("Escape", "Escape", 27, None),
        "space" => (" ", "Space", 32, Some(" ")),
        "backspace" => ("Backspace", "Backspace", 8, None),
        "delete" | "del" => ("Delete", "Delete", 46, None),
        "up" => ("ArrowUp", "ArrowUp", 38, None),
        "down" => ("ArrowDown", "ArrowDown", 40, None),
        "left" => ("ArrowLeft", "ArrowLeft", 37, None),
        "right" => ("ArrowRight", "ArrowRight", 39, None),
        "home" => ("Home", "Home", 36, None),
        "end" => ("End", "End", 35, None),
        "pageup" => ("PageUp", "PageUp", 33, None),
        "pagedown" => ("PageDown", "PageDown", 34, None),
        _ => return None,
    })
}

/// Editing commands Chrome runs for these shortcuts; without them a
/// protocol `ctrl+a` reaches the page's handlers but selects nothing.
fn command(mods: i64, key: char) -> Option<&'static str> {
    if mods & (CTRL | META) == 0 || mods & ALT != 0 {
        return None;
    }
    Some(match (key, mods & SHIFT != 0) {
        ('a', false) => "selectAll",
        ('c', false) => "copy",
        ('x', false) => "cut",
        ('v', false) => "paste",
        ('z', false) => "undo",
        ('z', true) | ('y', false) => "redo",
        _ => return None,
    })
}

/// Turn a validated chord into what the lane sends. Errors on keys the lane
/// cannot express (function keys other than F5).
pub fn chord(keys: &[String]) -> anyhow::Result<Chord> {
    let Some((last, mods)) = keys.split_last() else {
        anyhow::bail!("no key");
    };
    let mut m = 0;
    for k in mods {
        m |= match k.as_str() {
            "ctrl" => CTRL,
            "alt" => ALT,
            "shift" => SHIFT,
            "cmd" => META,
            other => anyhow::bail!("{other:?} is not a modifier"),
        };
    }
    let k = last.as_str();
    match (m, k) {
        (ALT, "left") => return Ok(Chord::Back),
        (ALT, "right") => return Ok(Chord::Forward),
        (0, "f5") | (CTRL, "r") => return Ok(Chord::Reload),
        _ => {}
    }
    let (key, code, vk, text, cmd) = if let Some((key, code, vk, text)) = named(k) {
        (
            key.to_string(),
            code.to_string(),
            vk,
            text.map(str::to_string),
            None,
        )
    } else {
        let mut chars = k.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            anyhow::bail!("the browser lane cannot press {k:?}");
        };
        let lower = c.to_ascii_lowercase();
        let upper = c.to_ascii_uppercase();
        let code = match c {
            'a'..='z' | 'A'..='Z' => format!("Key{upper}"),
            '0'..='9' => format!("Digit{c}"),
            _ => String::new(),
        };
        let vk = if c.is_ascii_alphanumeric() {
            upper as i64
        } else {
            0
        };
        let shown = if m & SHIFT != 0 { upper } else { c };
        // Typed characters only when no command modifier is held.
        let text = (m & (CTRL | ALT | META) == 0).then(|| shown.to_string());
        (shown.to_string(), code, vk, text, command(m, lower))
    };
    let mut down = json!({
        "type": if text.is_some() { "keyDown" } else { "rawKeyDown" },
        "modifiers": m,
        "key": key,
        "code": code,
        "windowsVirtualKeyCode": vk,
    });
    if let Some(t) = text {
        down["text"] = json!(t);
    }
    if let Some(c) = cmd {
        down["commands"] = json!([c]);
    }
    Ok(Chord::Keys(vec![down]))
}

/// The `keyUp` that ends a `keyDown`.
pub fn key_up(down: &Value) -> Value {
    let mut up = down.clone();
    if let Some(o) = up.as_object_mut() {
        o.remove("text");
        o.remove("commands");
        o.insert("type".into(), json!("keyUp"));
    }
    up
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(keys: &[&str]) -> Chord {
        chord(&keys.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    fn down(c: Chord) -> Value {
        match c {
            Chord::Keys(v) => v[0].clone(),
            other => panic!("expected keys, got {other:?}"),
        }
    }

    #[test]
    fn enter_types_a_carriage_return() {
        let d = down(k(&["enter"]));
        assert_eq!(d["type"], "keyDown");
        assert_eq!(d["key"], "Enter");
        assert_eq!(d["windowsVirtualKeyCode"], 13);
        assert_eq!(d["text"], "\r");
        let up = key_up(&d);
        assert_eq!(up["type"], "keyUp");
        assert!(up.get("text").is_none());
    }

    #[test]
    fn ctrl_a_carries_the_select_all_command_and_no_text() {
        let d = down(k(&["ctrl", "a"]));
        assert_eq!(d["type"], "rawKeyDown");
        assert_eq!(d["modifiers"], CTRL);
        assert_eq!(d["code"], "KeyA");
        assert_eq!(d["commands"], json!(["selectAll"]));
        assert!(d.get("text").is_none());
    }

    #[test]
    fn plain_and_shifted_characters_type_text() {
        assert_eq!(down(k(&["x"]))["text"], "x");
        assert_eq!(down(k(&["shift", "x"]))["text"], "X");
        assert_eq!(down(k(&["7"]))["code"], "Digit7");
    }

    #[test]
    fn browser_shortcuts_become_navigation() {
        assert_eq!(k(&["alt", "left"]), Chord::Back);
        assert_eq!(k(&["alt", "right"]), Chord::Forward);
        assert_eq!(k(&["f5"]), Chord::Reload);
        assert_eq!(k(&["ctrl", "r"]), Chord::Reload);
    }

    #[test]
    fn unknown_keys_and_modifiers_are_errors() {
        let s = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(chord(&s(&["f9"])).is_err());
        assert!(chord(&s(&["enter", "a"])).is_err());
        assert!(chord(&[]).is_err());
    }
}
