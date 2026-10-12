//! The real hands: [`DesktopActuator`] implements kai-core's Actuator for the
//! machine Kai runs on (the eyes, [`DesktopPerceptor`], live in `perceive`).

use crate::{a11y, input::Input, kill::KillSwitch, launch, shell::Shell};
use async_trait::async_trait;
use kai_core::{Action, Actuator, Decision, Grounding};
use std::time::Duration;

pub use crate::perceive::DesktopPerceptor;

/// Mouse, keyboard, launcher and terminal.
pub struct DesktopActuator {
    pub input: Input,
    pub shell: Shell,
}

impl DesktopActuator {
    pub fn new(kill: KillSwitch) -> anyhow::Result<Self> {
        Ok(Self {
            input: Input::spawn()?,
            shell: Shell::new(kill),
        })
    }

    fn point(decision: &Decision, grounding: &Grounding) -> anyhow::Result<(i32, i32)> {
        let target = decision
            .target
            .ok_or_else(|| anyhow::anyhow!("no target"))?;
        let r = grounding
            .target_rect(target)
            .ok_or_else(|| anyhow::anyhow!("target #{target} is not on screen"))?;
        let (x, y) = r.center();
        Ok((x.round() as i32, y.round() as i32))
    }
}

#[async_trait]
impl Actuator for DesktopActuator {
    async fn act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        match d.action {
            Action::Click => {
                let (x, y) = Self::point(d, g)?;
                self.input.click(x, y, false, 1).await
            }
            Action::DoubleClick => {
                let (x, y) = Self::point(d, g)?;
                self.input.click(x, y, false, 2).await
            }
            Action::RightClick => {
                let (x, y) = Self::point(d, g)?;
                self.input.click(x, y, true, 1).await
            }
            Action::Type => {
                if let Ok((x, y)) = Self::point(d, g) {
                    self.input.click(x, y, false, 1).await?;
                    tokio::time::sleep(Duration::from_millis(60)).await;
                }
                self.input.type_text(d.text.as_deref().unwrap_or("")).await
            }
            Action::Key => self.input.keys(&d.keys).await,
            Action::Scroll => {
                let (x, y) = match Self::point(d, g) {
                    Ok(p) => p,
                    Err(_) => self.input.cursor().await?,
                };
                self.input
                    .scroll(x, y, d.direction.unwrap_or_default(), d.amount.unwrap_or(3))
                    .await
            }
            Action::OpenApp => self.open_app(d.text.as_deref().unwrap_or("")).await,
            Action::OpenUrl => self.open_url(d.text.as_deref().unwrap_or("")).await,
            Action::Shell => self.shell(d.text.as_deref().unwrap_or("")).await.map(drop),
            Action::Wait | Action::AskUser | Action::Done => Ok(()),
        }
    }

    /// Accessibility-first: AT-SPI Action/EditableText on Linux, UI
    /// Automation patterns on Windows, for the element the latest observation
    /// grounded under this mark. `Ok(false)` means "use `act`". The caller
    /// asks for approval before calling this, exactly as before `act`.
    async fn a11y_act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<bool> {
        a11y::act(d, g).await
    }

    async fn open_app(&self, app: &str) -> anyhow::Result<()> {
        let app = app.to_string();
        tokio::task::spawn_blocking(move || launch::open_app(&app)).await?
    }

    async fn open_url(&self, url: &str) -> anyhow::Result<()> {
        let url = url.to_string();
        tokio::task::spawn_blocking(move || launch::open_url(&url)).await?
    }

    async fn shell(&self, cmd: &str) -> anyhow::Result<String> {
        if kills_kai(cmd, &own_pids()) {
            return Ok(
                "Not run: this command would stop Kai itself (it names Kai's own process). \
                 Kai is not the app you are looking for; find the target app's own process."
                    .to_string(),
            );
        }
        // `mousepad file.txt` as a shell step would block until the timeout
        // (a GUI app never exits by itself): start it detached instead.
        #[cfg(target_os = "linux")]
        if crate::launch::linux::is_gui_command(cmd) {
            std::process::Command::new("setsid")
                .args(["-f", "sh", "-c", cmd])
                .current_dir(std::env::var_os("HOME").unwrap_or_else(|| "/".into()))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            return Ok(format!(
                "started `{cmd}` as an app in the background (its window should open)\n[exit 0]"
            ));
        }
        self.shell.run(cmd).await
    }
}

/// Kai's own process and its parent (the app, or the terminal running
/// `kai`).
fn own_pids() -> Vec<u32> {
    let mut v = vec![std::process::id()];
    #[cfg(unix)]
    v.push(std::os::unix::process::parent_id());
    v
}

/// Would `cmd` kill Kai? A kill-family command (`kill`, `pkill`, `killall`,
/// `taskkill`, `Stop-Process`) that names one of `own` as a number, targets
/// every process (`kill -1`), or matches Kai by name. Seen live: Gemma ran
/// `ps aux | grep mousepad`, which matched Kai's own command line, then
/// killed that PID.
pub fn kills_kai(cmd: &str, own: &[u32]) -> bool {
    let c = cmd.to_ascii_lowercase();
    let words: Vec<&str> = c
        .split(|ch: char| ch.is_whitespace() || ";|&()`'\"".contains(ch))
        .filter(|w| !w.is_empty())
        .collect();
    let killer = |w: &str| {
        let w = w.rsplit('/').next().unwrap_or(w);
        matches!(
            w,
            "kill" | "pkill" | "killall" | "taskkill" | "stop-process"
        )
    };
    let Some(start) = words.iter().position(|w| killer(w)) else {
        return false;
    };
    words[start + 1..].iter().any(|w| {
        w.trim_start_matches('-')
            .parse::<u32>()
            .is_ok_and(|n| own.contains(&n))
            || *w == "-1"
            || w.starts_with("kai")
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_command_that_would_kill_kai_is_refused() {
        let own = [31093, 400];
        for cmd in [
            "kill 31093",
            "kill -9 31093",
            "ps aux | grep mousepad; kill 31093",
            "/bin/kill -TERM 400",
            "kill -9 -1",
            "pkill kai",
            "killall kai-app",
            "taskkill /PID 31093 /F",
            "Stop-Process -Id 31093",
        ] {
            assert!(super::kills_kai(cmd, &own), "{cmd}");
        }
        for cmd in [
            "kill 1234",
            "pkill mousepad",
            "ls -la 31093",
            "killall mousepad",
        ] {
            assert!(!super::kills_kai(cmd, &own), "{cmd}");
        }
    }

    use super::*;
    use kai_core::{Element, Rect, Role};

    #[test]
    fn click_point_is_element_centre() {
        let g = Grounding::Marks(vec![Element {
            mark: 4,
            role: Role::Button,
            name: "x".into(),
            bounds: Rect::new(10.0, 20.0, 30.0, 40.0),
            enabled: true,
            secure: false,
            focused: false,
            value: None,
        }]);
        let d =
            Decision::parse_checked(r#"{"thought":"","action":"click","target":4,"risk":"none"}"#)
                .unwrap();
        assert_eq!(DesktopActuator::point(&d, &g).unwrap(), (25, 40));
        let missing =
            Decision::parse_checked(r#"{"thought":"","action":"click","target":9,"risk":"none"}"#)
                .unwrap();
        assert!(DesktopActuator::point(&missing, &g).is_err());
    }
}
