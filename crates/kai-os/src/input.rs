//! Mouse and keyboard through `enigo`, on one dedicated thread so the OS input
//! handle is created once and never crosses threads. Text goes through the
//! input backend; Kai never replaces the user's clipboard to type it.

use enigo::{Axis, Button, Coordinate, Direction as Press, Enigo, Key, Keyboard, Mouse, Settings};
use kai_core::Direction;
use kai_core::computer::ExecutionContext;
mod execute;
#[cfg(any(target_os = "macos", test))]
mod mac_keys;
use execute::run;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

enum Cmd {
    Move {
        x: i32,
        y: i32,
    },
    Drag {
        points: Vec<(i32, i32)>,
        button: Button,
        duration_ms: u32,
    },
    Scroll2 {
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
    },
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
    /// Run `inner` with `keys` held (Shift+click), released afterwards.
    Held {
        keys: Vec<Key>,
        inner: Box<Cmd>,
    },
}

impl Cmd {
    /// Whether running this puts the pointer somewhere.
    fn moves_pointer(&self) -> bool {
        match self {
            Cmd::Type(_) | Cmd::Keys(_) | Cmd::Cursor => false,
            Cmd::Held { inner, .. } => inner.moves_pointer(),
            _ => true,
        }
    }
}

type Reply = mpsc::Sender<anyhow::Result<(i32, i32)>>;

/// Handle to the input thread. Cheap to clone.
#[derive(Clone)]
pub struct Input {
    tx: mpsc::Sender<(Cmd, Reply, Option<ExecutionContext>)>,
    context: Option<ExecutionContext>,
    /// Modifiers held around every command sent through this handle.
    held: Vec<Key>,
    /// Kai's own pointer commands, so the corner stop can tell them from
    /// the person's moves.
    moves: Arc<Moves>,
}

/// Pointer commands finished, and in flight, across every clone.
#[derive(Default)]
struct Moves {
    done: AtomicU64,
    busy: AtomicU64,
}

impl Input {
    pub async fn move_to(&self, x: i32, y: i32) -> anyhow::Result<()> {
        self.call(Cmd::Move { x, y }).await.map(drop)
    }

    pub async fn drag(
        &self,
        points: Vec<(i32, i32)>,
        button: kai_core::computer::MouseButton,
        duration_ms: u32,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            (2..=256).contains(&points.len()) && duration_ms <= 5000,
            "invalid drag"
        );
        self.call(Cmd::Drag {
            points,
            button: native_button(button),
            duration_ms,
        })
        .await
        .map(drop)
    }

    pub async fn click_button(
        &self,
        x: i32,
        y: i32,
        button: kai_core::computer::MouseButton,
        count: u8,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(matches!(count, 1 | 2), "invalid click count");
        self.call(Cmd::Click {
            x,
            y,
            button: native_button(button),
            count,
        })
        .await
        .map(drop)
    }

    pub async fn wheel(&self, x: i32, y: i32, dx: i32, dy: i32) -> anyhow::Result<()> {
        anyhow::ensure!(
            dx.unsigned_abs() <= 20 && dy.unsigned_abs() <= 20,
            "invalid wheel amount"
        );
        self.call(Cmd::Scroll2 { x, y, dx, dy }).await.map(drop)
    }
    pub fn spawn() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<(Cmd, Reply, Option<ExecutionContext>)>();
        let (ready_tx, ready_rx) = mpsc::channel();
        thread::Builder::new()
            .name("kai-input".into())
            .spawn(move || {
                // macOS: the app asks for the permission when the person
                // chooses to (`permissions::enable_accessibility`), not on
                // every retry here.
                let settings = Settings {
                    open_prompt_to_get_permissions: false,
                    ..Settings::default()
                };
                let mut enigo = match Enigo::new(&settings) {
                    Ok(e) => {
                        let _ = ready_tx.send(Ok(()));
                        e
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(anyhow::anyhow!("cannot simulate input: {e}")));
                        return;
                    }
                };
                for (cmd, reply, context) in rx {
                    let result = run(&mut enigo, cmd, context.as_ref());
                    let unsafe_state = result
                        .as_ref()
                        .err()
                        .is_some_and(|e| e.downcast_ref::<execute::CleanupFailed>().is_some());
                    let _ = reply.send(result);
                    if unsafe_state {
                        break;
                    } // Drop the queue: no input after failed cleanup.
                }
            })?;
        ready_rx.recv()??;
        Ok(Self {
            tx,
            context: None,
            held: Vec::new(),
            moves: Arc::default(),
        })
    }

    pub fn with_context(&self, context: &ExecutionContext) -> Self {
        Self {
            tx: self.tx.clone(),
            context: Some(context.clone()),
            held: self.held.clone(),
            moves: self.moves.clone(),
        }
    }

    /// The same handle, holding `modifiers` (canonical names) around each
    /// command it sends.
    pub fn holding(&self, modifiers: &[String]) -> anyhow::Result<Self> {
        #[cfg(target_os = "macos")]
        let modifiers = &mac_keys::held_to_mac(modifiers);
        Ok(Self {
            tx: self.tx.clone(),
            context: self.context.clone(),
            held: modifiers
                .iter()
                .map(|m| parse_key(m))
                .collect::<anyhow::Result<_>>()?,
            moves: self.moves.clone(),
        })
    }

    pub(crate) fn context(&self) -> Option<ExecutionContext> {
        self.context.clone()
    }

    /// How many pointer commands Kai has finished; changes after each one.
    pub fn pointer_moves(&self) -> u64 {
        self.moves.done.load(Ordering::SeqCst)
    }

    /// Whether a pointer command is running now (a drag takes seconds).
    pub fn pointer_busy(&self) -> bool {
        self.moves.busy.load(Ordering::SeqCst) > 0
    }

    async fn call(&self, cmd: Cmd) -> anyhow::Result<(i32, i32)> {
        if !cmd.moves_pointer() {
            return self.send(cmd).await;
        }
        self.moves.busy.fetch_add(1, Ordering::SeqCst);
        let result = self.send(cmd).await;
        self.moves.busy.fetch_sub(1, Ordering::SeqCst);
        self.moves.done.fetch_add(1, Ordering::SeqCst);
        result
    }

    async fn send(&self, cmd: Cmd) -> anyhow::Result<(i32, i32)> {
        let cmd = match self.held.is_empty() {
            true => cmd,
            false => Cmd::Held {
                keys: self.held.clone(),
                inner: Box::new(cmd),
            },
        };
        let (rtx, rrx) = mpsc::channel();
        self.tx
            .send((cmd, rtx, self.context.clone()))
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
        self.call(Cmd::Type(text.to_string())).await.map(drop)
    }

    pub async fn keys(&self, names: &[String]) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        let mac = mac_shortcut(names);
        #[cfg(target_os = "macos")]
        let names = mac.as_deref().unwrap_or(names);
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
}

/// The Mac form of a Windows/Linux shortcut (`mac_keys`), checked against
/// the app in front so a terminal keeps its `ctrl`.
#[cfg(target_os = "macos")]
fn mac_shortcut(names: &[String]) -> Option<Vec<String>> {
    let terminal = crate::a11y::frontmost_app_id().is_some_and(|id| mac_keys::is_terminal(&id));
    let mapped = mac_keys::to_mac(names, terminal)?;
    log::info!(
        "pressing {} for {} (the Mac shortcut)",
        mapped.join("+"),
        names.join("+")
    );
    Some(mapped)
}

fn native_button(button: kai_core::computer::MouseButton) -> Button {
    use kai_core::computer::MouseButton as B;
    match button {
        B::Left => Button::Left,
        B::Middle => Button::Middle,
        B::Right => Button::Right,
        B::Back => Button::Back,
        B::Forward => Button::Forward,
    }
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

#[cfg(test)]
mod tests;
