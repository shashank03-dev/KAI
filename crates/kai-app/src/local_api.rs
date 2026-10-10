//! The local-models contract as the app uses it, behind one small module.
//!
//! TODO(merge): replace with kai_core::models / kai_os::local. Everything
//! below the `pub use` line mirrors the shared contract by name (ModelEntry,
//! ModelFile, Catalog, HardwareSummary, Fit, Ranked, rank, detect_hardware,
//! LocalManager::{install, installed, remove, start, stop, status}, Progress,
//! LocalServer), so the swap is: delete this file's body and re-export the
//! real items. The stub never downloads or spawns anything; with
//! `KAI_LOCAL_DEMO=1` it pretends to, so the UI flow can be exercised.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelFile {
    pub url: String,
    pub sha256: Option<String>,
    /// "model" or "mmproj".
    pub role: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelEntry {
    pub id: String,
    pub family: String,
    pub label: String,
    pub maker: String,
    pub license: String,
    pub params_b: f32,
    pub active_params_b: Option<f32>,
    pub quant: String,
    pub vision: bool,
    pub context: u32,
    pub download_gb: f32,
    pub min_ram_gb: f32,
    pub min_vram_gb: Option<f32>,
    pub files: Vec<ModelFile>,
    pub runtime: String,
    pub served_name: String,
    pub notes: String,
    /// "recommended", "experimental" or "untested".
    pub computer_use: String,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HardwareSummary {
    pub ram_gb: f32,
    pub cpu_threads: u32,
    pub gpu_vram_gb: Option<f32>,
    pub gpu_name: Option<String>,
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Best,
    Good,
    Slow,
    TooBig,
}

#[derive(Debug, Clone)]
pub struct Ranked {
    pub entry: ModelEntry,
    pub fit: Fit,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    /// "runtime", "model", "verify" or "done".
    pub stage: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalServer {
    pub base_url: String,
    pub pid: u32,
}

/// The built-in catalog. TODO(merge): `kai_core::models` loads catalog.json.
pub fn load_catalog() -> Catalog {
    let m = |id: &str, label: &str, maker: &str, p: f32, a: Option<f32>, gb: f32, ram: f32| {
        ModelEntry {
            id: id.into(),
            family: id.split('-').take(2).collect::<Vec<_>>().join("-"),
            label: label.into(),
            maker: maker.into(),
            license: "Apache-2.0".into(),
            params_b: p,
            active_params_b: a,
            quant: "Q4_K_M".into(),
            vision: true,
            context: 32768,
            download_gb: gb,
            min_ram_gb: ram,
            min_vram_gb: Some(gb + 1.0),
            files: Vec::new(),
            runtime: "llama.cpp".into(),
            served_name: id.into(),
            notes: String::new(),
            computer_use: "untested".into(),
        }
    };
    let mut models = vec![
        m(
            "gemma-4-e4b-it-q4",
            "Gemma 4 E4B",
            "Google",
            8.0,
            Some(4.0),
            5.0,
            8.0,
        ),
        m(
            "gemma-4-e2b-it-q4",
            "Gemma 4 E2B",
            "Google",
            5.1,
            Some(2.0),
            3.2,
            6.0,
        ),
        m(
            "gemma-4-26b-a4b-it-q4",
            "Gemma 4 26B A4B",
            "Google",
            26.0,
            Some(4.0),
            16.0,
            24.0,
        ),
        m(
            "qwen2.5-vl-7b-q4",
            "Qwen2.5-VL 7B",
            "Alibaba Qwen",
            7.0,
            None,
            4.7,
            10.0,
        ),
        m(
            "ui-tars-1.5-7b-q4",
            "UI-TARS 1.5 7B",
            "ByteDance",
            7.0,
            None,
            4.7,
            10.0,
        ),
        m(
            "gemma-4-31b-it-q4",
            "Gemma 4 31B",
            "Google",
            31.0,
            None,
            19.0,
            32.0,
        ),
    ];
    models[0].computer_use = "recommended".into();
    models[1].computer_use = "experimental".into();
    models[2].computer_use = "recommended".into();
    models[4].computer_use = "experimental".into();
    Catalog { models }
}

/// TODO(merge): `kai_core::models::rank`.
pub fn rank(catalog: &Catalog, hw: &HardwareSummary) -> Vec<Ranked> {
    let mut out: Vec<Ranked> = catalog
        .models
        .iter()
        .map(|e| {
            let gpu = match (hw.gpu_vram_gb, e.min_vram_gb, &hw.gpu_name) {
                (Some(v), Some(need), Some(name)) if v >= need => {
                    Some(format!("runs on your {name} ({v:.0} GB)"))
                }
                _ => None,
            };
            let (fit, reason) = if e.min_ram_gb > hw.ram_gb {
                let reason = format!(
                    "Needs {:.0} GB RAM; you have {:.0} GB",
                    e.min_ram_gb, hw.ram_gb
                );
                (Fit::TooBig, reason)
            } else if let Some(g) = gpu {
                let reason = format!("Fits in your {:.0} GB RAM; {g}", hw.ram_gb);
                (Fit::Good, reason)
            } else if e.active_params_b.unwrap_or(e.params_b) > 4.5 {
                let reason = format!(
                    "Fits in your {:.0} GB RAM, but runs on the CPU only: slow",
                    hw.ram_gb
                );
                (Fit::Slow, reason)
            } else {
                let reason = format!("Fits in your {:.0} GB RAM; runs on the CPU", hw.ram_gb);
                (Fit::Good, reason)
            };
            Ranked {
                entry: e.clone(),
                fit,
                reason,
            }
        })
        .collect();
    let best = out
        .iter()
        .position(|r| r.fit == Fit::Good && r.entry.computer_use == "recommended")
        .or_else(|| out.iter().position(|r| r.fit != Fit::TooBig));
    if let Some(i) = best {
        out[i].fit = Fit::Best;
        let b = out.remove(i);
        out.insert(0, b);
    }
    let order = |f: Fit| match f {
        Fit::Best => 0,
        Fit::Good => 1,
        Fit::Slow => 2,
        Fit::TooBig => 3,
    };
    out.sort_by_key(|r| order(r.fit));
    out
}

/// TODO(merge): `kai_os::local::detect_hardware`.
pub fn detect_hardware() -> HardwareSummary {
    let ram_gb = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<f32>().ok())
        })
        .map(|kb| kb / 1024.0 / 1024.0)
        .unwrap_or(0.0);
    HardwareSummary {
        ram_gb,
        cpu_threads: std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
        gpu_vram_gb: None,
        gpu_name: None,
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
    }
}

/// Where local runtimes and models live. TODO(merge): kai_os's data dir.
pub fn local_root() -> PathBuf {
    kai_core::settings::Settings::dir().join("local")
}

/// TODO(merge): `kai_os::local::LocalManager`.
pub struct LocalManager {
    pub root: PathBuf,
    installed: Mutex<BTreeSet<String>>,
    running: Mutex<Option<LocalServer>>,
}

fn demo() -> bool {
    std::env::var("KAI_LOCAL_DEMO").is_ok_and(|v| v == "1")
}

const NOT_YET: &str = "Local models aren't in this build of Kai yet.";

impl LocalManager {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            installed: Mutex::new(BTreeSet::new()),
            running: Mutex::new(None),
        }
    }

    pub async fn install(
        &self,
        entry: &ModelEntry,
        progress: &dyn Fn(Progress),
    ) -> anyhow::Result<()> {
        anyhow::ensure!(demo(), NOT_YET);
        let total = (entry.download_gb * 1e9) as u64;
        for (stage, size) in [("runtime", 30_000_000u64), ("model", total)] {
            for i in 0..=20u64 {
                progress(Progress {
                    stage: stage.into(),
                    bytes: size * i / 20,
                    total: Some(size),
                    message: String::new(),
                });
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
        }
        progress(Progress {
            stage: "done".into(),
            ..Progress::default()
        });
        self.installed
            .lock()
            .expect("lock")
            .insert(entry.id.clone());
        Ok(())
    }

    pub fn installed(&self) -> Vec<String> {
        self.installed
            .lock()
            .expect("lock")
            .iter()
            .cloned()
            .collect()
    }

    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        self.installed.lock().expect("lock").remove(id);
        Ok(())
    }

    pub async fn start(&self, entry: &ModelEntry, port: u16) -> anyhow::Result<LocalServer> {
        anyhow::ensure!(demo(), NOT_YET);
        let _ = &entry.served_name;
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let server = LocalServer {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            pid: std::process::id(),
        };
        *self.running.lock().expect("lock") = Some(server.clone());
        Ok(server)
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        *self.running.lock().expect("lock") = None;
        Ok(())
    }

    pub fn status(&self) -> Option<LocalServer> {
        self.running.lock().expect("lock").clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(ram: f32, gpu: Option<f32>) -> HardwareSummary {
        HardwareSummary {
            ram_gb: ram,
            cpu_threads: 8,
            gpu_vram_gb: gpu,
            gpu_name: gpu.map(|_| "RTX 4060".into()),
            os: "linux".into(),
            arch: "x86_64".into(),
        }
    }

    #[test]
    fn exactly_one_best_comes_first() {
        let r = rank(&load_catalog(), &hw(16.0, Some(8.0)));
        assert_eq!(r.iter().filter(|r| r.fit == Fit::Best).count(), 1);
        assert_eq!(r[0].fit, Fit::Best);
        assert!(r.iter().any(|r| r.fit == Fit::TooBig));
    }

    #[test]
    fn nothing_fits_means_no_best() {
        let r = rank(&load_catalog(), &hw(2.0, None));
        assert!(r.iter().all(|r| r.fit == Fit::TooBig));
        assert!(r[0].reason.starts_with("Needs "));
    }
}
