//! The real hands: [`DesktopActuator`] implements kai-core's Actuator for the
//! machine Kai runs on (the eyes, [`DesktopPerceptor`], live in `perceive`).

use crate::{a11y, input::Input, kill::KillSwitch, launch, shell::Shell};
use async_trait::async_trait;
use kai_core::{Action, Actuator, Decision, Grounding};
use std::time::Duration;

pub use crate::perceive::DesktopPerceptor;

/// The longest wait for a started app or opened link to show a window.
const LAUNCH_WAIT: Duration = Duration::from_secs(10);

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

    /// Start an app or open a link, then wait for a window to show. `false`
    /// when the window list could be read and nothing changed.
    async fn launch(
        &self,
        target: &str,
        start: fn(&str) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        let target = target.to_string();
        let context = self.input.context();
        tokio::task::spawn_blocking(move || {
            let before = crate::window::shot();
            if let Some(ctx) = &context {
                ctx.check()?;
            }
            start(&target)?;
            Ok(crate::window::wait_for_change(&before, LAUNCH_WAIT))
        })
        .await?
    }

    /// Where to click before typing: nowhere for an untargeted `type` (it
    /// types at the focus), the target's centre otherwise. A target that is
    /// no longer on screen is an error, never a silent type at the focus.
    fn type_point(
        decision: &Decision,
        grounding: &Grounding,
    ) -> anyhow::Result<Option<(i32, i32)>> {
        match decision.target {
            None => Ok(None),
            Some(t) => Self::point(decision, grounding).map(Some).map_err(|_| {
                anyhow::anyhow!("target #{t} is not on the screen any more; observe again")
            }),
        }
    }
}

#[async_trait]
impl Actuator for DesktopActuator {
    async fn a11y_context(
        &self,
        d: &Decision,
        g: &Grounding,
        ctx: &kai_core::computer::ExecutionContext,
    ) -> anyhow::Result<bool> {
        a11y::act_context(d, g, ctx).await
    }

    async fn act_context(
        &self,
        d: &Decision,
        g: &Grounding,
        ctx: &kai_core::computer::ExecutionContext,
    ) -> anyhow::Result<Option<String>> {
        let scoped = Self {
            input: self.input.with_context(ctx),
            shell: self.shell.clone(),
        };
        ctx.check()?;
        match d.action {
            Action::OpenApp | Action::OpenUrl => {
                let target = d.text.as_deref().unwrap_or("");
                let start = match d.action {
                    Action::OpenApp => launch::open_app,
                    _ => launch::open_url,
                };
                let appeared = scoped.launch(target, start).await?;
                Ok(launch_note(target, appeared))
            }
            Action::Shell => scoped
                .shell(d.text.as_deref().unwrap_or(""))
                .await
                .map(Some),
            _ => scoped.act(d, g).await.map(|_| None),
        }
    }

    fn computer_capabilities(&self) -> kai_core::computer::Capabilities {
        crate::computer::capabilities()
    }
    async fn computer_act(
        &self,
        action: &kai_core::computer::ComputerAction,
        space: &kai_core::computer::ObservationSpace,
        grounding: &Grounding,
        context: &kai_core::computer::ExecutionContext,
    ) -> Result<(), kai_core::computer::ToolError> {
        crate::computer::act(self, action, space, grounding, context).await
    }
    async fn act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        match d.action {
            Action::Computer => anyhow::bail!("computer tools require versioned dispatch"),
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
                if let Some((x, y)) = Self::type_point(d, g)? {
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
        self.launch(app, launch::open_app).await.map(drop)
    }

    async fn open_url(&self, url: &str) -> anyhow::Result<()> {
        self.launch(url, launch::open_url).await.map(drop)
    }

    async fn shell(&self, cmd: &str) -> anyhow::Result<String> {
        if kills_kai(cmd, &own_pids()) {
            return Ok(
                "Not run: this command would stop Kai itself (it names Kai's own process). \
                 Kai is not the app you are looking for; find the target app's own process."
                    .to_string(),
            );
        }
        // `mousepad file.txt` or `notepad x.txt` as a shell step would block
        // until the timeout (a GUI app never exits by itself): start it
        // detached instead, on every OS.
        if crate::gui_launch::is_gui_command(cmd) {
            let _owner = kai_core::computer::mutation_owner().await;
            let before = tokio::task::spawn_blocking(crate::window::shot).await?;
            if let Some(ctx) = self.input.context() {
                ctx.check()?;
            }
            crate::gui_launch::spawn_detached(cmd)?;
            tokio::task::spawn_blocking(move || {
                crate::window::wait_for_change(&before, LAUNCH_WAIT)
            })
            .await?;
            return Ok(format!(
                "started `{cmd}` as an app in the background (its window should open)\n[exit 0]"
            ));
        }
        self.shell.run(cmd).await
    }
}

/// What the model hears after a launch that showed no new window, so it
/// does not click a screen that will never change (typo'd app names were
/// reported as opened).
fn launch_note(target: &str, appeared: bool) -> Option<String> {
    (!appeared).then(|| {
        format!(
            "no window opened or changed within {}s after opening {target:?}: \
             check the name, or it was already open and in front",
            LAUNCH_WAIT.as_secs()
        )
    })
}

/// Kai's own process and its parent (the app, or the terminal running
/// `kai`).
fn own_pids() -> Vec<u32> {
    // Only Unix adds the parent below (Windows CI: unused_mut).
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut v = vec![std::process::id()];
    #[cfg(unix)]
    v.push(std::os::unix::process::parent_id());
    v
}

/// Would `cmd` kill Kai? A kill-family command (`kill`, `pkill`, `killall`,
/// `taskkill`, `Stop-Process`) that names one of `own` as a number (or
/// `$PPID`, the shell's parent), targets every process (`kill -9 -1`), or
/// matches Kai by name (`pkill -f target/release/kai`), including through a
/// pipeline that feeds it (`pgrep kai | xargs kill`). Seen live: Gemma ran
/// `ps aux | grep mousepad`, which matched Kai's own command line, then
/// killed that PID. Only the statement holding the killer counts, so
/// `pkill mousepad; cat ~/kai/notes` runs.
pub fn kills_kai(cmd: &str, own: &[u32]) -> bool {
    // PID 1 cannot be killed by Kai's user, and is the parent of a detached
    // app: it must not make `kill -1 <pid>` (SIGHUP) look like a hit.
    let own: Vec<u32> = own.iter().copied().filter(|p| *p > 1).collect();
    let c = cmd.to_ascii_lowercase().replace("||", ";");
    c.split([';', '\n', '&'])
        .any(|statement| statement_kills(statement, &own))
}

fn statement_kills(statement: &str, own: &[u32]) -> bool {
    let words = |seg: &str| -> Vec<String> {
        seg.split(|ch: char| ch.is_whitespace() || "()`'\"{}".contains(ch))
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let segments: Vec<Vec<String>> = statement.split('|').map(words).collect();
    let killer = |w: &String| {
        let w = w.rsplit(['/', '\\']).next().unwrap_or(w);
        let w = w.strip_suffix(".exe").unwrap_or(w);
        matches!(
            w,
            "kill" | "pkill" | "killall" | "taskkill" | "stop-process"
        )
    };
    let Some((k, at)) = segments
        .iter()
        .enumerate()
        .find_map(|(k, ws)| ws.iter().position(killer).map(|at| (k, at)))
    else {
        return false;
    };
    let names_kai = |w: &str| w.split(['/', '\\']).any(|part| part.starts_with("kai"));
    let own_number = |w: &str| {
        w.strip_prefix('-')
            .unwrap_or(w)
            .parse::<u32>()
            .is_ok_and(|n| own.contains(&n))
    };
    let hits = kill_targets(&segments[k][at + 1..])
        .iter()
        .any(|w| own_number(w) || *w == "-1" || matches!(*w, "$ppid" | "${ppid}") || names_kai(w));
    // What a pipeline feeds the killer (`pgrep kai | xargs kill`).
    let fed = segments[..k]
        .iter()
        .flatten()
        .filter(|w| !w.starts_with('-'))
        .any(|w| own_number(w) || names_kai(w));
    hits || fed
}

/// The arguments of a kill command that name what to kill: a leading
/// signal (`-9`, `-TERM`, `-1` before a PID) and `-s`/`-n` with their value
/// are left out; after `--` everything counts.
fn kill_targets(args: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut options = true;
    while i < args.len() {
        let a = args[i].as_str();
        if options && a == "--" {
            options = false;
        } else if options && matches!(a, "-s" | "-n" | "--signal" | "-signal") {
            i += 1;
        } else if !(options && i == 0 && a.starts_with('-') && args.len() > 1) {
            out.push(a);
        }
        i += 1;
    }
    out
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

    #[test]
    fn the_kill_guard_sees_pipelines_paths_and_signals() {
        // PID 1 is the parent of a detached app (`setsid -f kai-app`).
        let own = [31093, 1];
        for cmd in [
            "pgrep kai | xargs kill",
            "pgrep -f kai-app | xargs -r kill -9",
            "pkill -f target/release/kai",
            "pkill -f \"/home/user/KAI/target/release/kai-app\"",
            "echo 31093 | xargs kill",
            "kill -s TERM 31093",
            "kill -- -1",
            "kill -9 -31093",
            "kill $PPID",
            "Get-Process kai | Stop-Process",
            "taskkill.exe /IM kai.exe /F",
            "cd /tmp && kill 31093",
        ] {
            assert!(super::kills_kai(cmd, &own), "{cmd}");
        }
        for cmd in [
            // `-1` before a PID is SIGHUP, and PID 1 is not Kai.
            "kill -1 1234",
            "kill -9 1",
            "kill %1",
            "kill -l",
            "pkill mousepad; cat ~/kai/notes.txt",
            "kill 1234 && ls kai.txt",
            "ps aux | grep mousepad | awk '{print $2}' | xargs kill",
            "pkill -x mousepad || true",
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

    #[test]
    fn a_launch_with_no_new_window_says_so() {
        assert_eq!(launch_note("Safari", true), None);
        let note = launch_note("Safariii", false).unwrap();
        assert!(
            note.contains("\"Safariii\"") && note.contains("check the name"),
            "{note}"
        );
    }

    #[test]
    fn typing_at_a_vanished_target_is_refused_not_typed_at_the_focus() {
        let g = Grounding::Marks(Vec::new());
        let gone = Decision::parse_checked(
            r#"{"thought":"","action":"type","target":9,"text":"hi","risk":"none"}"#,
        )
        .unwrap();
        let err = DesktopActuator::type_point(&gone, &g).unwrap_err();
        assert!(err.to_string().contains("#9"), "{err}");
        let mut untargeted = gone.clone();
        untargeted.target = None;
        assert_eq!(DesktopActuator::type_point(&untargeted, &g).unwrap(), None);
    }
}
