//! Mouse and keyboard through `enigo`, on one dedicated thread so the OS input
//! handle is created once and never crosses threads. Long text is pasted
//! through the clipboard, and the user's clipboard is put back afterwards.

use enigo::{Axis, Button, Coordinate, Direction as Press, Enigo, Key, Keyboard, Mouse, Settings};
use kai_core::Direction;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Text longer than this is pasted instead of typed key by key.
pub const PASTE_OVER: usize = 32;

enum Cmd {
    Click {
        x: i32,
        y: i32,
        button: Button,
        count: u8,
    },
    Type(String),
    Keys(Vec<Key>),
    Scroll {
        x: i32,
        y: i32,
        dir: Direction,
        amount: i32,
    },
    Cursor,
}

type Reply = mpsc::Sender<anyhow::Result<(i32, i32)>>;

/// Handle to the input thread. Cheap to clone.
#[derive(Clone)]
pub struct Input {
    tx: mpsc::Sender<(Cmd, Reply)>,
}

impl Input {
    pub fn spawn() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<(Cmd, Reply)>();
        let (ready_tx, ready_rx) = mpsc::channel();
        thread::Builder::new()
            .name("kai-input".into())
            .spawn(move || {
                let mut enigo = match Enigo::new(&Settings::default()) {
                    Ok(e) => {
                        let _ = ready_tx.send(Ok(()));
                        e
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(anyhow::anyhow!("cannot simulate input: {e}")));
                        return;
                    }
                };
                for (cmd, reply) in rx {
                    let _ = reply.send(run(&mut enigo, cmd));
                }
            })?;
        ready_rx.recv()??;
        Ok(Self { tx })
    }

    async fn call(&self, cmd: Cmd) -> anyhow::Result<(i32, i32)> {
        let (rtx, rrx) = mpsc::channel();
        self.tx
            .send((cmd, rtx))
            .map_err(|_| anyhow::anyhow!("input thread stopped"))?;
        tokio::task::spawn_blocking(move || {
            rrx.recv()
                .map_err(|_| anyhow::anyhow!("input thread stopped"))?
        })
        .await?
    }

    pub async fn click(&self, x: i32, y: i32, right: bool, count: u8) -> anyhow::Result<()> {
        let button = if right { Button::Right } else { Button::Left };
        self.call(Cmd::Click {
            x,
            y,
            button,
            count,
        })
        .await
        .map(drop)
    }

    pub async fn type_text(&self, text: &str) -> anyhow::Result<()> {
        if text.chars().count() > PASTE_OVER {
            return self.paste(text).await;
        }
        self.call(Cmd::Type(text.to_string())).await.map(drop)
    }

    pub async fn keys(&self, names: &[String]) -> anyhow::Result<()> {
        let keys = names
            .iter()
            .map(|n| parse_key(n))
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.call(Cmd::Keys(keys)).await.map(drop)
    }

    pub async fn scroll(&self, x: i32, y: i32, dir: Direction, amount: u32) -> anyhow::Result<()> {
        self.call(Cmd::Scroll {
            x,
            y,
            dir,
            amount: amount as i32,
        })
        .await
        .map(drop)
    }

    /// Where the pointer is, for the top-left-corner kill gesture.
    pub async fn cursor(&self) -> anyhow::Result<(i32, i32)> {
        self.call(Cmd::Cursor).await
    }

    async fn paste(&self, text: &str) -> anyhow::Result<()> {
        let saved = clipboard_get();
        clipboard_set(text)?;
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        let r = self.keys(&[modifier.to_string(), "v".to_string()]).await;
        // Give the target app time to read the clipboard before restoring it.
        tokio::time::sleep(Duration::from_millis(250)).await;
        if let Some(old) = saved {
            let _ = clipboard_set(&old);
        }
        r
    }
}

fn run(e: &mut Enigo, cmd: Cmd) -> anyhow::Result<(i32, i32)> {
    match cmd {
        Cmd::Click {
            x,
            y,
            button,
            count,
        } => {
            e.move_mouse(x, y, Coordinate::Abs)?;
            thread::sleep(Duration::from_millis(30));
            for _ in 0..count.max(1) {
                e.button(button, Press::Click)?;
            }
        }
        Cmd::Type(t) => e.text(&t)?,
        Cmd::Keys(keys) => {
            let (mods, last) = keys.split_at(keys.len().saturating_sub(1));
            for k in mods {
                e.key(*k, Press::Press)?;
            }
            for k in last {
                e.key(*k, Press::Click)?;
            }
            for k in mods.iter().rev() {
                e.key(*k, Press::Release)?;
            }
        }
        Cmd::Scroll { x, y, dir, amount } => {
            e.move_mouse(x, y, Coordinate::Abs)?;
            let (len, axis) = match dir {
                Direction::Down => (amount, Axis::Vertical),
                Direction::Up => (-amount, Axis::Vertical),
                Direction::Right => (amount, Axis::Horizontal),
                Direction::Left => (-amount, Axis::Horizontal),
            };
            e.scroll(len, axis)?;
        }
        Cmd::Cursor => return Ok(e.location()?),
    }
    Ok((0, 0))
}

/// Map a normalised key name (see `Decision::validate_and_clamp`) to enigo.
pub fn parse_key(name: &str) -> anyhow::Result<Key> {
    let k = match name {
        "ctrl" => Key::Control,
        "alt" => Key::Alt,
        "shift" => Key::Shift,
        "cmd" => Key::Meta,
        "enter" => Key::Return,
        "esc" => Key::Escape,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Unicode(c),
                _ => anyhow::bail!("unknown key {other:?}"),
            }
        }
    };
    Ok(k)
}

fn clipboard_get() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

fn clipboard_set(text: &str) -> anyhow::Result<()> {
    arboard::Clipboard::new()?.set_text(text.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_and_single_keys() {
        assert!(matches!(parse_key("ctrl").unwrap(), Key::Control));
        assert!(matches!(parse_key("enter").unwrap(), Key::Return));
        assert!(matches!(parse_key("s").unwrap(), Key::Unicode('s')));
        assert!(matches!(parse_key("f5").unwrap(), Key::F5));
        assert!(parse_key("hyperdrive").is_err());
    }
}
