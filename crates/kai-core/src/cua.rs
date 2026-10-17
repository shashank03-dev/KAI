//! Responses API comparison adapter, sharing Kai's engine and policy.
use crate::client::ClientError;
use crate::ports::{
    Actuator, ApprovalChoice, Approver, EventSink, Interrupt, NoSteering, Perceptor,
};
use crate::{Decision, Grounding, Trace};
use std::time::Duration;
mod mapping;
mod provider;
mod turn;
pub use mapping::{Mapped, map_action};
/// Settings for one native run.
#[derive(Debug, Clone)]
pub struct CuaConfig {
    /// e.g. `https://api.openai.com/v1`.
    pub base: String,
    /// `OPENAI_API_KEY`. Required: the AI Gateway does not serve
    /// `computer-use-preview` (it answers 404 model_not_found, measured).
    pub api_key: Option<String>,
    pub model: String,
    /// Current Responses computer protocol; legacy preview remains default.
    pub computer_tools_v1: bool,
    /// Optional Responses reasoning effort for GA computer models. Never
    /// sent to the legacy computer-use-preview API.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Opt-in local Phase 5 safety checks for native computer actions.
    pub local_safety_checks: bool,
    /// `linux` or `windows`.
    pub environment: String,
    pub max_steps: u32,
    pub max_time: Duration,
    pub timeout: Duration,
}

/// Explicit reasoning levels. Model support varies; callers opt in only
/// after selecting a compatible GA model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningEffort {
    None,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }
}

/// The computer-use tool's `environment`: `windows`, `mac` or `linux`.
fn cua_environment() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

impl Default for CuaConfig {
    fn default() -> Self {
        Self {
            base: crate::client::OPENAI_BASE.to_string(),
            api_key: None,
            model: "computer-use-preview".to_string(),
            computer_tools_v1: false,
            reasoning_effort: None,
            local_safety_checks: false,
            environment: cua_environment().to_string(),
            max_steps: 40,
            max_time: Duration::from_secs(600),
            timeout: Duration::from_secs(60),
        }
    }
}

/// What a native run produced: the trace (scored like Kai's), and the error
/// that ended it early, if any.
#[derive(Debug)]
pub struct CuaRun {
    pub trace: Trace,
    pub error: Option<String>,
    /// The model's last text message (its summary or question), if any.
    pub message: Option<String>,
}

/// The native loop's collaborators.
pub struct CuaRunner<'a> {
    pub config: CuaConfig,
    pub perceptor: &'a dyn Perceptor,
    pub actuator: &'a dyn Actuator,
    pub interrupt: &'a dyn Interrupt,
    pub sink: &'a dyn EventSink,
}

impl CuaRunner<'_> {
    pub async fn run(&self, task: &str) -> Result<CuaRun, ClientError> {
        self.run_with_approver(task, None).await
    }
    pub async fn run_with_approver(
        &self,
        task: &str,
        approver: Option<&dyn Approver>,
    ) -> Result<CuaRun, ClientError> {
        self.run_with_monitor(task, approver, None).await
    }

    /// An independent screenshot classifier, selected explicitly by the
    /// caller. No classifier is constructed or called unless supplied.
    /// Its flags and errors use the shared engine's present-person safety
    /// approval path before the native computer action can run.
    pub async fn run_with_monitor(
        &self,
        task: &str,
        approver: Option<&dyn Approver>,
        monitor: Option<&dyn crate::client::ModelClient>,
    ) -> Result<CuaRun, ClientError> {
        let approver = approver.unwrap_or(&NoApproval);
        let client = provider::Provider::new(
            &self.config,
            task,
            self.perceptor,
            approver,
            self.actuator.computer_capabilities(),
        )?;
        let engine = crate::Engine {
            grounder: None,
            client: &client,
            careful: None,
            perceptor: self.perceptor,
            actuator: self.actuator,
            interrupt: self.interrupt,
            sink: self.sink,
            approver,
            steering: &NoSteering,
            verifier: None,
            human: None,
            skills: &[],
            config: crate::Config {
                computer_tools_v1: true,
                max_steps: self.config.max_steps,
                max_time: self.config.max_time,
                max_failures: 1,
                ..Default::default()
            },
        };
        let mut trace = engine
            .run_with(
                task,
                crate::RunOptions {
                    injection_monitor: monitor,
                    ..Default::default()
                },
            )
            .await;
        if trace.errors.iter().any(|e| e.starts_with("model error")) {
            trace.finish(crate::Outcome::Failed);
        }
        let (message, error) = client.finish().await;
        Ok(CuaRun {
            trace,
            message,
            error,
        })
    }
}
/// Without an approver nothing that needs one happens: every coordinate
/// action is high risk, so the run stops at its first.
struct NoApproval;
#[async_trait::async_trait]
impl Approver for NoApproval {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        ApprovalChoice::Stop
    }
}

/// For a sandboxed desktop with nobody to ask (the bench), passed
/// explicitly: approves coordinate actions, but stops where the native loop
/// always stopped: a password, a secure field or sensitive data typed.
pub struct Unattended;
#[async_trait::async_trait]
impl Approver for Unattended {
    async fn approve(&self, d: &Decision, g: &Grounding) -> ApprovalChoice {
        if crate::risk::is_payment_or_password(d, g) {
            ApprovalChoice::Stop
        } else {
            ApprovalChoice::Approve
        }
    }
    /// A provider's safety check is a person's call, never the bench's.
    async fn approve_safety_check(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        ApprovalChoice::Stop
    }
}

/// Width and height from a JPEG's SOF marker.
pub fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if (0xC0..=0xCF).contains(&m) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unattended_runs_go_on_but_never_type_secrets() {
        let g = Grounding::Marks(vec![]);
        let click =
            Decision::parse_checked(r#"{"thought":"x","action":"click","target":1,"risk":"high"}"#)
                .unwrap();
        assert_eq!(
            Unattended.approve(&click, &g).await,
            ApprovalChoice::Approve
        );
        let card = Decision::parse_checked(
            r#"{"thought":"x","action":"type","target":1,"text":"4111 1111 1111 1111","risk":"low"}"#,
        )
        .unwrap();
        assert_eq!(Unattended.approve(&card, &g).await, ApprovalChoice::Stop);
    }
}
