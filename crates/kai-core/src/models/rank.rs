//! "Best for this computer": which catalog models fit this machine, how
//! fast they will feel, and one plain-English reason each. The rules and
//! their thresholds are documented in `docs/LOCAL_MODELS.md` and pinned by
//! the hardware table in `rank_tests.rs`.
//!
//! Memory model: an entry's `min_ram_gb` / `min_vram_gb` already hold files +
//! KV cache + runtime overhead. On top, the OS and the user's apps keep
//! [`os_reserve_gb`] of RAM, and the desktop keeps [`GPU_RESERVE_GB`] of
//! graphics memory.

use super::{Catalog, HardwareSummary, ModelEntry};

/// How well a model fits. Exactly one entry is `Best` when anything fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fit {
    Best,
    Good,
    Slow,
    TooBig,
}

impl Fit {
    pub fn label(self) -> &'static str {
        match self {
            Fit::Best => "Best for this computer",
            Fit::Good => "Runs well",
            Fit::Slow => "Runs slowly",
            Fit::TooBig => "Too big",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Ranked {
    pub entry: ModelEntry,
    pub fit: Fit,
    pub reason: String,
}

/// Graphics memory the desktop keeps (compositor, browser), GB.
pub const GPU_RESERVE_GB: f32 = 0.5;
/// A split between GPU and RAM counts as comfortable when the two together
/// hold this much more than the model needs.
pub const SPLIT_COMFORT: f32 = 1.25;
/// A split runs well only when the GPU holds at least this share.
pub const SPLIT_MIN_GPU_SHARE: f32 = 0.3;
/// Models with at most this many billion parameters working per token are
/// usable on a CPU or split (E2B, E4B, and the 26B A4B mixture of experts).
pub const SMALL_ACTIVE_B: f32 = 5.0;

/// RAM the OS and other apps keep: a quarter, at least 2 GB, at most 6 GB.
pub fn os_reserve_gb(ram_gb: f32) -> f32 {
    (ram_gb * 0.25).clamp(2.0, 6.0)
}

/// The smallest machine RAM (whole GB) that leaves `need` after the reserve.
pub fn ram_needed_gb(need: f32) -> f32 {
    let r = if need + 2.0 <= 8.0 {
        need + 2.0
    } else if need / 0.75 <= 24.0 {
        need / 0.75
    } else {
        need + 6.0
    };
    r.ceil()
}

/// Where the model would run.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Place {
    Gpu,
    Split { comfortable: bool },
    Cpu,
    Nowhere,
}

fn place(e: &ModelEntry, hw: &HardwareSummary) -> Place {
    let ram = (hw.ram_gb - os_reserve_gb(hw.ram_gb)).max(0.0);
    let vram = hw
        .gpu_vram_gb
        .map(|v| (v - GPU_RESERVE_GB).max(0.0))
        .unwrap_or(0.0);
    let need = e.min_ram_gb;
    let need_gpu = e.min_vram_gb.unwrap_or(need);
    if vram > 0.0 && vram >= need_gpu {
        return Place::Gpu;
    }
    // A Mac's GPU memory is part of its RAM: there is nothing to split across.
    if hw.os == "macos" {
        return if ram >= need {
            Place::Cpu
        } else {
            Place::Nowhere
        };
    }
    if vram >= SPLIT_MIN_GPU_SHARE * need && vram + ram >= need {
        return Place::Split {
            comfortable: vram + ram >= need * SPLIT_COMFORT,
        };
    }
    if ram >= need {
        Place::Cpu
    } else if vram > 0.0 && vram + ram >= need {
        Place::Split { comfortable: false }
    } else {
        Place::Nowhere
    }
}

/// Higher is better: 3 runs well, 2 usable on the CPU, 1 tight, 0 very slow,
/// -1 does not fit (or cannot see the screen).
fn tier(e: &ModelEntry, p: Place) -> i32 {
    if !e.vision {
        return -1;
    }
    let small = e.active_b() <= SMALL_ACTIVE_B;
    match p {
        Place::Gpu => 3,
        Place::Split { comfortable: true } if small => 3,
        Place::Cpu if small => 2,
        Place::Split { comfortable: false } if small => 1,
        Place::Split { .. } | Place::Cpu => 0,
        Place::Nowhere => -1,
    }
}

fn computer_use_rank(e: &ModelEntry) -> i32 {
    match e.computer_use.as_str() {
        "recommended" => 2,
        "experimental" => 1,
        _ => 0,
    }
}

/// Ties between equally tested models go to the ones made for computer use.
fn agent_rank(e: &ModelEntry) -> i32 {
    i32::from(e.agent_trained)
}

fn gpu_name(hw: &HardwareSummary) -> String {
    let name = hw.gpu_name.clone().unwrap_or_else(|| "GPU".into());
    match hw.gpu_vram_gb {
        Some(v) => format!("{name} ({v:.0} GB)"),
        None => name,
    }
}

fn reason(e: &ModelEntry, hw: &HardwareSummary, p: Place) -> String {
    let ram = hw.ram_gb;
    let gpu = gpu_name(hw);
    let active = e.active_b();
    if !e.vision {
        return "Cannot see the screen (no vision); Kai needs a vision model".into();
    }
    match p {
        Place::Gpu => format!("Fits in your {ram:.0} GB RAM; runs fully on your {gpu}"),
        Place::Split { comfortable: true } if active <= SMALL_ACTIVE_B => {
            let moe = if e.active_params_b.is_some() && e.params_b > 2.0 * active {
                format!("; only {active:.1}B parameters work per token, so it stays quick")
            } else {
                String::new()
            };
            format!("Runs on your {gpu} with the rest in your {ram:.0} GB RAM{moe}")
        }
        Place::Split { .. } if active <= SMALL_ACTIVE_B => format!(
            "Tight fit: needs {:.0} GB across your {gpu} and {ram:.0} GB RAM; slow, \
             and leaves little memory free",
            e.min_ram_gb
        ),
        Place::Split { .. } => format!(
            "Fits across your {gpu} and {ram:.0} GB RAM, but a dense {:.0}B model split \
             this way is very slow",
            e.params_b
        ),
        Place::Cpu if active <= SMALL_ACTIVE_B => {
            let why = if hw.gpu_vram_gb.is_none() {
                "no dedicated GPU found, so it runs on the CPU"
            } else {
                "runs on the CPU"
            };
            format!(
                "Fits in your {ram:.0} GB RAM; {why} ({} threads), expect tens of seconds \
                 per step",
                hw.cpu_threads
            )
        }
        Place::Cpu => format!(
            "Fits in your {ram:.0} GB RAM, but this {:.0}B dense model is very slow on the CPU",
            e.params_b
        ),
        Place::Nowhere => match hw.gpu_vram_gb {
            Some(v) => format!(
                "Needs {:.0} GB of memory; your {v:.0} GB GPU and {ram:.0} GB RAM are not enough",
                e.min_ram_gb
            ),
            None => format!(
                "Needs {:.0} GB RAM; you have {ram:.0} GB",
                ram_needed_gb(e.min_ram_gb)
            ),
        },
    }
}

/// Rank the catalog for this hardware: Best first, then by how well each
/// runs; models that do not fit last. Ties go to computer-use status, then
/// to models trained for computer use, then to the larger model.
pub fn rank(catalog: &Catalog, hw: &HardwareSummary) -> Vec<Ranked> {
    let mut scored: Vec<((i32, i32, i32), f32, Ranked)> = catalog
        .models
        .iter()
        .map(|e| {
            let p = place(e, hw);
            let t = tier(e, p);
            let fit = match t {
                3 => Fit::Good,
                0..=2 => Fit::Slow,
                _ => Fit::TooBig,
            };
            let r = Ranked {
                entry: e.clone(),
                fit,
                reason: reason(e, hw, p),
            };
            ((t, computer_use_rank(e), agent_rank(e)), e.params_b, r)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.total_cmp(&a.1)));
    let mut out: Vec<Ranked> = scored.into_iter().map(|s| s.2).collect();
    // "Best" is a recommendation, so it goes only to a model that has been
    // tried with Kai's harness; an untested model can rank high on fit
    // alone but is never recommended. No tested model fits: no Best.
    if let Some(i) = out
        .iter()
        .position(|r| r.fit != Fit::TooBig && computer_use_rank(&r.entry) > 0)
    {
        let mut best = out.remove(i);
        best.fit = Fit::Best;
        out.insert(0, best);
    }
    out
}
