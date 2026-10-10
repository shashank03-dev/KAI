//! `kai models list|install|start|stop|remove`, `--backend local`, and the
//! local-models part of `kai doctor`. Nothing is downloaded or started
//! unless one of these commands asks for it.

use clap::Subcommand;
use kai_core::models::{Catalog, DEFAULT_PORT, Fit, ModelEntry, rank};
use kai_core::settings::Settings;
use kai_os::local::{Cancelled, LocalManager, LocalServer, Progress, detect_hardware};
use std::io::Write;
use std::process::ExitCode;

#[derive(Subcommand)]
pub enum ModelsCmd {
    /// Open models for this computer, best first, with the reason for each.
    List,
    /// Download the runtime and a model (`best` = the recommended one).
    /// Resumes an interrupted download; Ctrl-C cancels and keeps the part.
    Install { id: String },
    /// Run a model's local server on 127.0.0.1 until Ctrl-C (or --detach).
    Start {
        id: String,
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Leave the server running in the background; `kai models stop` ends it.
        #[arg(long)]
        detach: bool,
    },
    /// Stop the local server Kai started.
    Stop,
    /// Delete a downloaded model.
    Remove { id: String },
}

pub async fn run(cmd: ModelsCmd) -> anyhow::Result<ExitCode> {
    let cat = Catalog::load();
    let m = LocalManager::open_default();
    match cmd {
        ModelsCmd::List => {
            list(&cat, &m);
            Ok(ExitCode::SUCCESS)
        }
        ModelsCmd::Install { id } => {
            let e = resolve(&cat, &id)?;
            install(&m, &e).await
        }
        ModelsCmd::Start { id, port, detach } => {
            let e = resolve(&cat, &id)?;
            let e = m.installed_entry(&e.id).unwrap_or(e);
            eprintln!(
                "Starting {} on 127.0.0.1:{port} (loading can take a minute)…",
                e.label
            );
            let mut server = m.start(&e, port).await?;
            println!("{} is ready at {}", e.label, server.base_url);
            println!(
                "Use it: kai run --backend local --model {} \"<task>\"",
                e.id
            );
            if detach {
                server.detach();
                println!("Running in the background; `kai models stop` ends it.");
                return Ok(ExitCode::SUCCESS);
            }
            eprintln!("Press Ctrl-C to stop.");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                r = server.wait() => { r?; eprintln!("the server exited"); }
            }
            server.stop().await?;
            Ok(ExitCode::SUCCESS)
        }
        ModelsCmd::Stop => {
            if m.stop()? {
                println!("Stopped the local model server.");
            } else {
                println!("No local model server is running.");
            }
            Ok(ExitCode::SUCCESS)
        }
        ModelsCmd::Remove { id } => {
            anyhow::ensure!(m.is_installed(&id), "{id} is not installed");
            m.remove(&id)?;
            println!("Removed {id}.");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// A catalog id, or `best` for this computer's recommendation.
fn resolve(cat: &Catalog, id: &str) -> anyhow::Result<ModelEntry> {
    if id == "best" {
        return rank(cat, &detect_hardware())
            .into_iter()
            .find(|r| r.fit == Fit::Best)
            .map(|r| r.entry)
            .ok_or_else(|| anyhow::anyhow!("no model fits this computer"));
    }
    cat.get(id).cloned().ok_or_else(|| {
        anyhow::anyhow!("{id} is not in the catalog; `kai models list` shows the ids")
    })
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

fn list(cat: &Catalog, m: &LocalManager) {
    let hw = detect_hardware();
    println!("This computer: {}\n", hw.describe());
    let installed = m.installed();
    for r in rank(cat, &hw) {
        let e = &r.entry;
        let mark = match r.fit {
            Fit::Best => "* BEST",
            Fit::Good => "  good",
            Fit::Slow => "  slow",
            Fit::TooBig => "  too big",
        };
        let have = if installed.contains(&e.id) {
            " [installed]"
        } else {
            ""
        };
        println!(
            "{mark:<9} {:<24} {}{have}\n          {} · {} · {:.1} GB · {} · vision: {} · computer use: {}\n          {}\n",
            e.id,
            e.label,
            e.maker,
            e.quant,
            e.download_gb,
            e.license,
            if e.vision { "yes" } else { "no" },
            e.computer_use,
            r.reason
        );
    }
    println!("Install one: kai models install <id>   (or: kai models install best)");
}

async fn install(m: &LocalManager, e: &ModelEntry) -> anyhow::Result<ExitCode> {
    let left = m.bytes_to_download(e)?;
    eprintln!(
        "Installing {} ({} to download) into {}",
        e.label,
        gb(left),
        m.root().display()
    );
    let canceller = m.clone();
    let ctrl_c = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            canceller.cancel();
        }
    });
    let show = |p: Progress| {
        let pct = p
            .total
            .filter(|t| *t > 0)
            .map(|t| format!("{:>5.1}%", p.bytes as f64 * 100.0 / t as f64))
            .unwrap_or_default();
        let mut err = std::io::stderr();
        let _ = write!(
            err,
            "\r\x1b[2K[{}] {pct} {} ({})",
            p.stage,
            p.message,
            gb(p.bytes)
        );
        if p.stage == "done" || p.stage == "verify" {
            let _ = writeln!(err);
        }
        let _ = err.flush();
    };
    let r = m.install(e, &show).await;
    ctrl_c.abort();
    match r {
        Ok(()) => {
            println!("Installed. Start it: kai models start {}", e.id);
            Ok(ExitCode::SUCCESS)
        }
        Err(err) if err.downcast_ref::<Cancelled>().is_some() => {
            eprintln!("\nCancelled. Run the same command again to resume.");
            Ok(ExitCode::FAILURE)
        }
        Err(err) => Err(err),
    }
}

/// For `--backend local`: make sure the chosen model's server is up. Uses a
/// running server of the same model, else starts one for this run (stopped
/// when the returned handle drops).
pub async fn ensure_local(s: &mut Settings) -> anyhow::Result<Option<LocalServer>> {
    let m = LocalManager::open_default();
    if s.local_model.is_empty() {
        s.local_model = m.installed().into_iter().next().ok_or_else(|| {
            anyhow::anyhow!(
                "no local model installed: `kai models list`, then `kai models install <id>`"
            )
        })?;
    }
    if let Some(st) = m.status().await
        && st.info.id == s.local_model
        && st.healthy
    {
        s.local_port = st.info.port;
        return Ok(None);
    }
    let e = m
        .installed_entry(&s.local_model)
        .or_else(|| Catalog::load().get(&s.local_model).cloned())
        .ok_or_else(|| anyhow::anyhow!("{} is not in the catalog", s.local_model))?;
    eprintln!("Starting {} on this computer…", e.label);
    let server = m.start(&e, s.local_port).await?;
    Ok(Some(server))
}

/// `kai doctor`'s local-models lines.
pub async fn doctor_report() -> Vec<String> {
    let m = LocalManager::open_default();
    let hw = detect_hardware();
    let mut v = vec![
        format!("hardware   {}", hw.describe()),
        format!("folder     {}", m.root().display()),
    ];
    let installed = m.installed();
    v.push(if installed.is_empty() {
        "installed  none (`kai models list`)".into()
    } else {
        format!("installed  {}", installed.join(", "))
    });
    v.push(match m.server_path() {
        Some(p) => format!("runtime    {}", p.display()),
        None => "runtime    not installed (comes with the first model)".into(),
    });
    v.push(match m.status().await {
        Some(st) => format!(
            "server     {} at {} ({})",
            st.info.id,
            st.info.base_url,
            if st.healthy { "ready" } else { "loading" }
        ),
        None => "server     not running".into(),
    });
    if let Some(best) = rank(&Catalog::load(), &hw)
        .into_iter()
        .find(|r| r.fit == Fit::Best)
    {
        v.push(format!("best       {} — {}", best.entry.id, best.reason));
    }
    v
}
