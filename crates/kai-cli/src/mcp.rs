//! `kai mcp`: Kai's eyes and hands as an MCP server on stdio
//! (`kai_core::mcp`, `docs/MCP.md`). Logs go to stderr; stdout carries only
//! protocol messages, one per line.

use crate::backend::{self, BackendOpts};
use async_trait::async_trait;
use kai_core::engine::{ApprovalChoice, Approver, Engine, NoSteering, NullSink};
use kai_core::mcp::{Server, TaskRunner};
use kai_core::{ApprovalLevel, Decision, Grounding, RunOptions, Trace};
use serde_json::{Value, json};
use std::process::ExitCode;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Nobody can approve over MCP: every step that needs approval is skipped,
/// and the report says so.
struct SkipRisky;

#[async_trait]
impl Approver for SkipRisky {
    async fn approve(&self, _d: &Decision, _g: &Grounding) -> ApprovalChoice {
        ApprovalChoice::Skip
    }
}

struct Runner<'a>(Engine<'a>);

#[async_trait]
impl TaskRunner for Runner<'_> {
    async fn run(&self, task: &str) -> Trace {
        let opts = RunOptions {
            probe: Some(&kai_os::DesktopProbe),
            ..Default::default()
        };
        let t = self.0.run_with(task, opts).await;
        crate::memory::save_trace(&t);
        t
    }
}

pub async fn serve(o: &BackendOpts) -> anyhow::Result<ExitCode> {
    let kill = kai_os::KillSwitch::new();
    let screen = kai_os::DesktopPerceptor::new();
    let hands = kai_os::DesktopActuator::new(kill.clone())?;
    // A model is only needed for kai_run_task; without one the other tools
    // still work.
    let mut model = None;
    let mut _local = None;
    match crate::live_settings(o) {
        Ok((b, mut s, keys)) => {
            _local = crate::runs::local_server(b, &mut s).await.ok().flatten();
            match (backend::config(&s, &o.ab), s.clients(&keys)) {
                (Ok((mut config, _)), Ok(clients)) => {
                    // Nobody can approve here, so autopilot would let high-risk
                    // steps run unasked: they are skipped instead.
                    if config.approval == ApprovalLevel::Autopilot {
                        config.approval = ApprovalLevel::HighRiskOnly;
                    }
                    model = Some((config, clients));
                }
                (Err(e), _) | (_, Err(e)) => eprintln!("kai mcp: no kai_run_task: {e}"),
            }
        }
        Err(e) => eprintln!("kai mcp: no kai_run_task: {e}"),
    }
    let skills = crate::all_skills();
    let approver = SkipRisky;
    let runner = model.as_ref().map(|(config, (fast, careful))| {
        Runner(Engine {
            grounder: None,
            client: fast.as_ref(),
            careful: careful.as_deref(),
            perceptor: &screen,
            actuator: &hands,
            approver: &approver,
            interrupt: &kill,
            sink: &NullSink,
            steering: &NoSteering,
            verifier: None,
            human: None,
            skills: &skills,
            config: *config,
        })
    });
    let server = Server::new(
        &screen,
        &hands,
        Some(&kai_os::DesktopProbe),
        runner.as_ref().map(|r| r as &dyn TaskRunner),
    );
    eprintln!("kai mcp: ready on stdio");
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => server.handle(&msg).await,
            Err(e) => Some(json!({"jsonrpc": "2.0", "id": null,
                "error": {"code": -32700, "message": format!("parse error: {e}")}})),
        };
        if let Some(r) = reply {
            out.write_all(format!("{r}\n").as_bytes()).await?;
            out.flush().await?;
        }
    }
    Ok(ExitCode::SUCCESS)
}
