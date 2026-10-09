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
        self.shell.run(cmd).await
    }
}

#[cfg(test)]
mod tests {
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
