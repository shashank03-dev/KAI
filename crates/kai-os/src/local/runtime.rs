//! The pinned llama.cpp `llama-server` build per OS, CPU architecture and
//! GPU, and unpacking it. Asset names, sizes and SHA-256 digests are from the
//! release page of [`LLAMA_CPP_TAG`] (2026-10-05); see docs/LOCAL_MODELS.md
//! for which were re-hashed here and which come from GitHub's digest only.

use kai_core::models::HardwareSummary;
use std::path::{Path, PathBuf};

/// The pinned llama.cpp release (published 2026-10-05).
pub const LLAMA_CPP_TAG: &str = "b11429";
const RELEASE_BASE: &str = "https://github.com/ggml-org/llama.cpp/releases/download";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Cpu,
    Vulkan,
    /// NVIDIA CUDA 12.4 (Windows only; needs a driver for CUDA 12).
    Cuda,
}

impl Variant {
    pub fn name(self) -> &'static str {
        match self {
            Variant::Cpu => "cpu",
            Variant::Vulkan => "vulkan",
            Variant::Cuda => "cuda",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeAsset {
    pub url: String,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
}

impl RuntimeAsset {
    pub fn file_name(&self) -> String {
        self.url.rsplit('/').next().unwrap_or_default().to_string()
    }
}

/// One runtime build: its archives (unpacked into one folder) and the
/// server's file name inside them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSpec {
    /// Folder name under `<root>/runtime/`, e.g. `llama.cpp-b11429-linux-x86_64-vulkan`.
    pub name: String,
    pub variant: Variant,
    pub assets: Vec<RuntimeAsset>,
    /// `llama-server` or `llama-server.exe`.
    pub server_exe: String,
}

impl RuntimeSpec {
    pub fn total_bytes(&self) -> u64 {
        self.assets.iter().filter_map(|a| a.size_bytes).sum()
    }
}

/// (os, arch, variant, [(asset, sha256, bytes)]).
type Row = (
    &'static str,
    &'static str,
    Variant,
    &'static [(&'static str, &'static str, u64)],
);

const TABLE: &[Row] = &[
    (
        "linux",
        "x86_64",
        Variant::Cpu,
        &[(
            "llama-b11429-bin-ubuntu-x64.tar.gz",
            "f6d25dde8f51133143d1453da4fd5f73b145127177612a283bf7995957af3392",
            17_693_462,
        )],
    ),
    (
        "linux",
        "x86_64",
        Variant::Vulkan,
        &[(
            "llama-b11429-bin-ubuntu-vulkan-x64.tar.gz",
            "632c4e98feba2b94407a2130e3133e0c3aefb0ea1ab41337e926d8bfafdd0b74",
            31_636_673,
        )],
    ),
    (
        "linux",
        "aarch64",
        Variant::Cpu,
        &[(
            "llama-b11429-bin-ubuntu-arm64.tar.gz",
            "ed44c0f79c3d02424f62bdc87e106318b0b89f49050e2e80c5f352723d14e00b",
            13_682_895,
        )],
    ),
    (
        "linux",
        "aarch64",
        Variant::Vulkan,
        &[(
            "llama-b11429-bin-ubuntu-vulkan-arm64.tar.gz",
            "702d99c4219b4314cc6d3b10fb41ae96cbfc68226305c2681269dfb51487af34",
            24_845_090,
        )],
    ),
    (
        "windows",
        "x86_64",
        Variant::Cpu,
        &[(
            "llama-b11429-bin-win-cpu-x64.zip",
            "1283323272b04cd07905816a597a0da810918102de958f4ff6f7bbaa70ed2efe",
            19_398_918,
        )],
    ),
    (
        "windows",
        "x86_64",
        Variant::Vulkan,
        &[(
            "llama-b11429-bin-win-vulkan-x64.zip",
            "1bfe78ad9168b79fa02bf67f6af9f5e17a966d824d77238517f7bef12ac73b36",
            33_337_769,
        )],
    ),
    (
        "windows",
        "x86_64",
        Variant::Cuda,
        &[
            (
                "llama-b11429-bin-win-cuda-12.4-x64.zip",
                "dd6df685c1024e6aa55ca55ad35038692d5e284d4dc895e7a646c3245d3d14ff",
                264_478_423,
            ),
            (
                "cudart-llama-bin-win-cuda-12.4-x64.zip",
                "8c79a9b226de4b3cacfd1f83d24f962d0773be79f1e7b75c6af4ded7e32ae1d6",
                391_443_627,
            ),
        ],
    ),
    (
        "windows",
        "aarch64",
        Variant::Cpu,
        &[(
            "llama-b11429-bin-win-cpu-arm64.zip",
            "ee0f631a9e58b146ff50714099d9cb498906af3143a773b580093a9214a8d1e5",
            12_229_035,
        )],
    ),
];

/// Which build suits this machine. Windows: CUDA only for an NVIDIA GPU,
/// else Vulkan for any other dedicated GPU, else CPU. Linux: Vulkan when a
/// dedicated GPU and the Vulkan loader (`libvulkan.so.1`) are present, else
/// CPU (the Linux CUDA build needs a 567 MB runtime or a system CUDA 12.8).
pub fn choose_variant(hw: &HardwareSummary, vulkan_loader: bool) -> Variant {
    let gpu = hw.gpu_vram_gb.is_some_and(|v| v >= 1.0);
    let nvidia = hw
        .gpu_name
        .as_deref()
        .is_some_and(|n| n.to_ascii_lowercase().contains("nvidia"));
    match hw.os.as_str() {
        "windows" if gpu && nvidia && hw.arch == "x86_64" => Variant::Cuda,
        "windows" if gpu && hw.arch == "x86_64" => Variant::Vulkan,
        "linux" if gpu && vulkan_loader => Variant::Vulkan,
        _ => Variant::Cpu,
    }
}

/// The pinned build for an OS, architecture and variant (falls back to the
/// CPU build when that variant does not exist there).
pub fn pinned(os: &str, arch: &str, variant: Variant) -> Option<RuntimeSpec> {
    let row = TABLE
        .iter()
        .find(|r| r.0 == os && r.1 == arch && r.2 == variant)
        .or_else(|| {
            TABLE
                .iter()
                .find(|r| r.0 == os && r.1 == arch && r.2 == Variant::Cpu)
        })?;
    Some(RuntimeSpec {
        name: format!("llama.cpp-{LLAMA_CPP_TAG}-{os}-{arch}-{}", row.2.name()),
        variant: row.2,
        assets: row
            .3
            .iter()
            .map(|(file, sha, size)| RuntimeAsset {
                url: format!("{RELEASE_BASE}/{LLAMA_CPP_TAG}/{file}"),
                sha256: Some(sha.to_string()),
                size_bytes: Some(*size),
            })
            .collect(),
        server_exe: if os == "windows" {
            "llama-server.exe".into()
        } else {
            "llama-server".into()
        },
    })
}

/// The build for this computer.
pub fn for_hardware(hw: &HardwareSummary) -> anyhow::Result<RuntimeSpec> {
    let v = choose_variant(hw, vulkan_loader_present());
    pinned(&hw.os, &hw.arch, v).ok_or_else(|| {
        anyhow::anyhow!(
            "no llama.cpp build for {} {}; Kai supports Linux and Windows on x86_64 and arm64",
            hw.os,
            hw.arch
        )
    })
}

/// Whether the Vulkan loader is installed (Linux).
pub fn vulkan_loader_present() -> bool {
    if cfg!(windows) {
        return true; // shipped with every current GPU driver
    }
    [
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/usr/lib64",
        "/usr/lib",
        "/lib/x86_64-linux-gnu",
        "/lib64",
    ]
    .iter()
    .any(|d| Path::new(d).join("libvulkan.so.1").exists())
}

/// Unpack a `.zip` or `.tar.gz` into `dir` (blocking; paths are confined to
/// `dir` by both libraries).
pub fn unpack(archive: &Path, dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let name = archive.to_string_lossy().to_ascii_lowercase();
    let f = std::fs::File::open(archive)?;
    if name.ends_with(".zip") {
        zip::ZipArchive::new(f)?.extract(dir)?;
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let mut a = tar::Archive::new(flate2::read::GzDecoder::new(f));
        a.set_preserve_permissions(true);
        a.unpack(dir)?;
    } else {
        anyhow::bail!("unknown archive type: {}", archive.display());
    }
    Ok(())
}

/// Find the server binary anywhere under `dir` (archives differ: the Linux
/// tarball has a `llama-<tag>/` folder, the Windows zips are flat).
pub fn find_server(dir: &Path, exe: &str) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).ok()?.flatten() {
            let p = e.path();
            let ft = e.file_type().ok()?;
            if ft.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == exe) {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(os: &str, gpu: Option<(&str, f32)>) -> HardwareSummary {
        HardwareSummary {
            ram_gb: 16.0,
            cpu_threads: 8,
            gpu_vram_gb: gpu.map(|g| g.1),
            gpu_name: gpu.map(|g| g.0.into()),
            os: os.into(),
            arch: "x86_64".into(),
        }
    }

    #[test]
    fn windows_picks_cuda_only_for_nvidia() {
        let nv = hw("windows", Some(("NVIDIA GeForce RTX 4060", 8.0)));
        assert_eq!(choose_variant(&nv, true), Variant::Cuda);
        let amd = hw("windows", Some(("AMD Radeon RX 7800 XT", 16.0)));
        assert_eq!(choose_variant(&amd, true), Variant::Vulkan);
        assert_eq!(choose_variant(&hw("windows", None), true), Variant::Cpu);
        let spec = pinned("windows", "x86_64", Variant::Cuda).unwrap();
        assert_eq!(spec.assets.len(), 2, "CUDA build plus its runtime DLLs");
        assert_eq!(spec.server_exe, "llama-server.exe");
    }

    #[test]
    fn linux_uses_vulkan_only_with_a_gpu_and_the_loader() {
        let nv = hw("linux", Some(("NVIDIA GeForce RTX 4060", 8.0)));
        assert_eq!(choose_variant(&nv, true), Variant::Vulkan);
        assert_eq!(choose_variant(&nv, false), Variant::Cpu);
        assert_eq!(choose_variant(&hw("linux", None), true), Variant::Cpu);
        let s = pinned("linux", "x86_64", Variant::Cuda).unwrap();
        assert_eq!(s.variant, Variant::Cpu, "no Linux CUDA row: falls back");
        assert!(pinned("macos", "aarch64", Variant::Cpu).is_none());
    }

    #[test]
    fn every_asset_is_pinned_and_hashed() {
        for r in TABLE {
            for (file, sha, size) in r.3 {
                assert!(file.contains(LLAMA_CPP_TAG) || file.starts_with("cudart-"));
                assert_eq!(sha.len(), 64);
                assert!(*size > 1_000_000);
            }
        }
        let s = pinned("linux", "x86_64", Variant::Vulkan).unwrap();
        assert_eq!(
            s.assets[0].url,
            "https://github.com/ggml-org/llama.cpp/releases/download/b11429/llama-b11429-bin-ubuntu-vulkan-x64.tar.gz"
        );
        assert_eq!(s.name, "llama.cpp-b11429-linux-x86_64-vulkan");
    }
}
