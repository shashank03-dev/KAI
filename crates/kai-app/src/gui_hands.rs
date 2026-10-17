//! Execution boundary for GUI acceptance runs. Shell requests fail closed.
use async_trait::async_trait;
use kai_core::computer::{
    Capabilities, ComputerAction, ExecutionContext, ObservationSpace, ToolError,
};
use kai_core::{Action, Actuator, Decision, Grounding};

pub struct GuiHands<'a>(pub &'a dyn Actuator);

fn blocked_app(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "terminal",
        "iterm",
        "warp",
        "alacritty",
        "wezterm",
        "kitty",
        "powershell",
        "cmd.exe",
        "script editor",
        "osascript",
        "visual studio code",
        "cursor",
        "zed",
    ]
    .iter()
    .any(|part| name.contains(part))
        || name == "code"
}

async fn foreground() -> anyhow::Result<()> {
    tokio::task::spawn_blocking(|| {
        let all = xcap::Window::all()?;
        let window = all.iter().find(|w| w.is_focused().unwrap_or(false))
            .ok_or_else(|| anyhow::anyhow!("GUI-only mode could not identify the focused app; focus a standard application and retry"))?;
        let app = window.app_name()?;
        let title = window.title().unwrap_or_default();
        anyhow::ensure!(!app.trim().is_empty() && !blocked_app(&app)
            && ![" — zsh", " — bash", " — fish", "powershell", "terminal"].iter().any(|s| title.to_ascii_lowercase().contains(s)),
            "GUI-only mode refuses input into terminals, script runners and code editors. Focus the application being tested.");
        Ok(())
    }).await??;
    Ok(())
}

async fn check(d: &Decision) -> anyhow::Result<()> {
    anyhow::ensure!(
        d.action != Action::Shell,
        "Shell execution is disabled for GUI-only runs. Use visible application controls."
    );
    if d.action == Action::OpenApp {
        anyhow::ensure!(
            !blocked_app(d.text.as_deref().unwrap_or("")),
            "GUI-only mode cannot launch terminals or script runners"
        );
    } else if !matches!(
        d.action,
        Action::OpenUrl | Action::Wait | Action::Done | Action::AskUser
    ) {
        foreground().await?;
    }
    Ok(())
}
#[async_trait]
impl Actuator for GuiHands<'_> {
    fn computer_capabilities(&self) -> Capabilities {
        self.0.computer_capabilities()
    }
    async fn computer_act(
        &self,
        a: &ComputerAction,
        s: &ObservationSpace,
        g: &Grounding,
        c: &ExecutionContext,
    ) -> Result<(), ToolError> {
        match a {
            ComputerAction::OpenApp { name } if blocked_app(name) => {
                return Err(ToolError::Unsupported);
            }
            ComputerAction::OpenApp { .. }
            | ComputerAction::OpenUrl { .. }
            | ComputerAction::Wait { .. }
            | ComputerAction::Observe => {}
            _ => foreground().await.map_err(|_| ToolError::Unsupported)?,
        }
        self.0.computer_act(a, s, g, c).await
    }
    async fn act_context(
        &self,
        d: &Decision,
        g: &Grounding,
        c: &ExecutionContext,
    ) -> anyhow::Result<Option<String>> {
        check(d).await?;
        self.0.act_context(d, g, c).await
    }
    async fn act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<()> {
        check(d).await?;
        self.0.act(d, g).await
    }
    async fn a11y_context(
        &self,
        d: &Decision,
        g: &Grounding,
        c: &ExecutionContext,
    ) -> anyhow::Result<bool> {
        check(d).await?;
        self.0.a11y_context(d, g, c).await
    }
    async fn a11y_act(&self, d: &Decision, g: &Grounding) -> anyhow::Result<bool> {
        check(d).await?;
        self.0.a11y_act(d, g).await
    }
    async fn open_app(&self, app: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !blocked_app(app),
            "GUI-only mode cannot launch terminals or script runners"
        );
        self.0.open_app(app).await
    }
    async fn open_url(&self, url: &str) -> anyhow::Result<()> {
        self.0.open_url(url).await
    }
    async fn shell(&self, _: &str) -> anyhow::Result<String> {
        anyhow::bail!("Shell execution is disabled for GUI-only runs")
    }
}
