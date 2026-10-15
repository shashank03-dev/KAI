//! The recommendation, pinned on a table of hardware profiles. Each row is
//! one rule from `docs/LOCAL_MODELS.md`.

use super::rank::{Fit, os_reserve_gb, ram_needed_gb, rank};
use super::{Catalog, HardwareSummary};

fn hw(ram: f32, threads: u32, gpu: Option<(&str, f32)>) -> HardwareSummary {
    HardwareSummary {
        ram_gb: ram,
        cpu_threads: threads,
        gpu_vram_gb: gpu.map(|g| g.1),
        gpu_name: gpu.map(|g| g.0.to_string()),
        os: "linux".into(),
        arch: "x86_64".into(),
    }
}

/// (profile, RAM GB, threads, GPU, expected Best id or None).
#[allow(clippy::type_complexity)]
const PROFILES: &[(&str, f32, u32, Option<(&str, f32)>, Option<&str>)] = &[
    ("tiny VM, 4 GB, no GPU", 4.0, 2, None, None),
    (
        "old laptop, 8 GB, no GPU",
        8.0,
        4,
        None,
        Some("gemma-4-e2b-it-q4"),
    ),
    (
        "laptop, 16 GB, no GPU",
        16.0,
        8,
        None,
        Some("gemma-4-e4b-it-q4"),
    ),
    (
        "desktop, 32 GB, no GPU",
        32.0,
        16,
        None,
        Some("gemma-4-26b-a4b-it-q4"),
    ),
    (
        "laptop, 16 GB + 4 GB GPU",
        16.0,
        8,
        Some(("RTX 3050 Laptop", 4.0)),
        Some("gemma-4-e4b-it-q4"),
    ),
    (
        "gaming PC, 16 GB + 8 GB GPU",
        16.0,
        12,
        Some(("RTX 4060", 8.0)),
        Some("gemma-4-e4b-it-q4"),
    ),
    (
        "gaming PC, 32 GB + 8 GB GPU",
        32.0,
        16,
        Some(("RTX 4060", 8.0)),
        Some("gemma-4-26b-a4b-it-q4"),
    ),
    (
        "workstation, 16 GB + 12 GB GPU",
        16.0,
        16,
        Some(("RTX 3060", 12.0)),
        Some("gemma-4-26b-a4b-it-q4"),
    ),
    (
        "workstation, 32 GB + 16 GB GPU",
        32.0,
        16,
        Some(("RTX 4080", 16.0)),
        Some("gemma-4-26b-a4b-it-q4"),
    ),
    (
        "big GPU, 64 GB + 24 GB GPU",
        64.0,
        24,
        Some(("RTX 4090", 24.0)),
        Some("gemma-4-31b-it-q4"),
    ),
    (
        "server, 128 GB + 48 GB GPU",
        128.0,
        64,
        Some(("RTX 6000 Ada", 48.0)),
        Some("gemma-4-31b-it-q4"),
    ),
];

#[test]
fn hardware_table() {
    let cat = Catalog::builtin();
    for (name, ram, threads, gpu, want) in PROFILES {
        let r = rank(&cat, &hw(*ram, *threads, *gpu));
        let best: Vec<_> = r.iter().filter(|x| x.fit == Fit::Best).collect();
        match want {
            Some(id) => {
                assert_eq!(best.len(), 1, "{name}: exactly one Best");
                assert_eq!(best[0].entry.id, *id, "{name}: {}", best[0].reason);
                assert_eq!(r[0].fit, Fit::Best, "{name}: Best comes first");
            }
            None => assert!(best.is_empty(), "{name}: nothing should fit"),
        }
        // Too big always sorts last, and every entry has a reason.
        let first_too_big = r
            .iter()
            .position(|x| x.fit == Fit::TooBig)
            .unwrap_or(r.len());
        assert!(
            r[first_too_big..].iter().all(|x| x.fit == Fit::TooBig),
            "{name}"
        );
        assert!(r.iter().all(|x| !x.reason.is_empty()));
        assert_eq!(r.len(), cat.models.len());
    }
}

#[test]
fn reasons_are_plain_english() {
    let cat = Catalog::builtin();
    let r = rank(&cat, &hw(16.0, 12, Some(("RTX 4060", 8.0))));
    let e4b = r
        .iter()
        .find(|x| x.entry.id == "gemma-4-e4b-it-q4")
        .unwrap();
    assert_eq!(
        e4b.reason,
        "Fits in your 16 GB RAM; runs fully on your RTX 4060 (8 GB)"
    );
    let r = rank(&cat, &hw(16.0, 8, None));
    let big = r
        .iter()
        .find(|x| x.entry.id == "gemma-4-26b-a4b-it-q4")
        .unwrap();
    assert_eq!(big.fit, Fit::TooBig);
    assert_eq!(big.reason, "Needs 24 GB RAM; you have 16 GB");
    let e4b = r
        .iter()
        .find(|x| x.entry.id == "gemma-4-e4b-it-q4")
        .unwrap();
    assert!(
        e4b.reason.contains("no dedicated GPU found"),
        "{}",
        e4b.reason
    );
    let r = rank(&cat, &hw(32.0, 16, Some(("RTX 4060", 8.0))));
    let moe = r
        .iter()
        .find(|x| x.entry.id == "gemma-4-26b-a4b-it-q4")
        .unwrap();
    assert!(
        moe.reason.contains("only 3.8B parameters work per token"),
        "{}",
        moe.reason
    );
}

#[test]
fn dense_models_off_the_gpu_are_slow_not_best() {
    let cat = Catalog::builtin();
    // 64 GB RAM, no GPU: the 31B fits in RAM but must not be recommended.
    let r = rank(&cat, &hw(64.0, 32, None));
    assert_eq!(r[0].entry.id, "gemma-4-26b-a4b-it-q4");
    let dense = r
        .iter()
        .find(|x| x.entry.id == "gemma-4-31b-it-q4")
        .unwrap();
    assert_eq!(dense.fit, Fit::Slow);
    assert!(dense.reason.contains("very slow"));
}

#[test]
fn untested_models_never_beat_gemma_at_the_same_speed() {
    let cat = Catalog::builtin();
    for (_, ram, threads, gpu, _) in PROFILES {
        let r = rank(&cat, &hw(*ram, *threads, *gpu));
        if let Some(b) = r.iter().find(|x| x.fit == Fit::Best) {
            assert_eq!(b.entry.family, "gemma-4");
        }
    }
}

#[test]
fn non_vision_models_are_never_best() {
    let mut cat = Catalog::builtin();
    for m in &mut cat.models {
        m.vision = false;
    }
    let r = rank(&cat, &hw(64.0, 32, Some(("RTX 4090", 24.0))));
    assert!(r.iter().all(|x| x.fit == Fit::TooBig));
}

#[test]
fn reserve_and_needed_ram() {
    assert_eq!(os_reserve_gb(4.0), 2.0);
    assert_eq!(os_reserve_gb(16.0), 4.0);
    assert_eq!(os_reserve_gb(64.0), 6.0);
    assert_eq!(ram_needed_gb(4.9), 7.0);
    assert_eq!(ram_needed_gb(17.4), 24.0);
    assert_eq!(ram_needed_gb(22.3), 29.0);
    // Round trip: the stated RAM really leaves the model's need.
    for need in [4.9f32, 6.9, 17.4, 22.3] {
        let r = ram_needed_gb(need);
        assert!(r - os_reserve_gb(r) >= need - 1e-3, "{need}");
    }
}

/// A Mac's GPU memory is part of its RAM: a model runs on the GPU or not,
/// never "on the GPU with the rest in RAM" (that would count memory twice).
#[test]
fn macs_never_split_unified_memory() {
    let cat = Catalog::builtin();
    for (ram, gpu) in [(16.0, 10.67), (24.0, 16.0), (32.0, 21.33)] {
        let mut mac = hw(ram, 10, Some(("Apple M5 GPU", gpu)));
        mac.os = "macos".into();
        mac.arch = "aarch64".into();
        let r = rank(&cat, &mac);
        assert_eq!(r[0].fit, Fit::Best, "{ram} GB Mac: {}", r[0].reason);
        if ram == 16.0 {
            assert_eq!(r[0].entry.id, "gemma-4-e4b-it-q4", "{}", r[0].reason);
            assert!(r[0].reason.contains("runs fully on your Apple M5 GPU"));
        }
        assert!(
            r.iter().all(|x| !x.reason.contains("with the rest in")),
            "{ram} GB Mac"
        );
    }
}
