//! What this computer has: RAM, CPU threads, and the biggest GPU's dedicated
//! memory. Best effort and quick; never downloads or installs anything.
//!
//! - Linux: `/proc/meminfo`; `nvidia-smi` when installed; AMD cards through
//!   `/sys/class/drm/card*/device/mem_info_vram_total`.
//! - Windows: `GlobalMemoryStatusEx`; DXGI adapters' `DedicatedVideoMemory`.
//!
//! Sizes are GiB (2^30 bytes), the unit the OS and GPU tools report.

use kai_core::models::HardwareSummary;

const GIB: f64 = (1u64 << 30) as f64;
/// Below this a "dedicated" memory figure is an integrated GPU's carve-out.
const MIN_VRAM_GIB: f32 = 1.0;

pub fn detect_hardware() -> HardwareSummary {
    let (gpu_name, gpu_vram_gb) = match gpu() {
        Some((n, v)) if v >= MIN_VRAM_GIB => (Some(n), Some(v)),
        _ => (None, None),
    };
    HardwareSummary {
        ram_gb: ram_bytes().map(|b| (b as f64 / GIB) as f32).unwrap_or(0.0),
        cpu_threads: std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1),
        gpu_vram_gb,
        gpu_name,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

/// `nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits`
/// output → the card with the most memory (name, GiB).
pub fn parse_nvidia_smi(out: &str) -> Option<(String, f32)> {
    out.lines()
        .filter_map(|l| {
            let (name, mib) = l.rsplit_once(',')?;
            let mib: f32 = mib.trim().parse().ok()?;
            Some((name.trim().to_string(), mib / 1024.0))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// `MemTotal` from `/proc/meminfo`, bytes.
pub fn parse_meminfo(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// Run a system tool with a time limit; its stdout when it succeeded.
#[allow(dead_code)]
fn run_quick(cmd: &str, args: &[&str]) -> Option<String> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut c = Command::new(cmd);
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = c.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(s) if s.success() => break,
            Some(_) => return None,
            None if start.elapsed() > Duration::from_secs(5) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut out = String::new();
    use std::io::Read;
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    Some(out)
}

#[cfg(target_os = "linux")]
fn ram_bytes() -> Option<u64> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(target_os = "linux")]
fn gpu() -> Option<(String, f32)> {
    let nvidia = run_quick(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ],
    )
    .and_then(|o| parse_nvidia_smi(&o));
    let amd = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter_map(|e| {
            let dev = e.path().join("device");
            let bytes: u64 = std::fs::read_to_string(dev.join("mem_info_vram_total"))
                .ok()?
                .trim()
                .parse()
                .ok()?;
            let name = std::fs::read_to_string(dev.join("product_name"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "AMD GPU".into());
            Some((name, (bytes as f64 / GIB) as f32))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1));
    [nvidia, amd]
        .into_iter()
        .flatten()
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

#[cfg(windows)]
fn ram_bytes() -> Option<u64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `m` is a properly sized, initialised MEMORYSTATUSEX.
    unsafe { GlobalMemoryStatusEx(&mut m) }.ok()?;
    Some(m.ullTotalPhys)
}

#[cfg(windows)]
fn gpu() -> Option<(String, f32)> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
    };
    // SAFETY: plain COM calls; every returned interface is owned and dropped.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    let mut best: Option<(String, f32)> = None;
    for i in 0.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else {
            break;
        };
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(128);
        let name = String::from_utf16_lossy(&desc.Description[..len]);
        let gib = (desc.DedicatedVideoMemory as f64 / GIB) as f32;
        if best.as_ref().is_none_or(|b| gib > b.1) {
            best = Some((name.trim().to_string(), gib));
        }
    }
    best
}

#[cfg(not(any(target_os = "linux", windows)))]
fn ram_bytes() -> Option<u64> {
    None
}

#[cfg(not(any(target_os = "linux", windows)))]
fn gpu() -> Option<(String, f32)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_output() {
        let out = "NVIDIA GeForce RTX 4060, 8188\nNVIDIA RTX A6000, 49140\n";
        let (n, v) = parse_nvidia_smi(out).unwrap();
        assert_eq!(n, "NVIDIA RTX A6000");
        assert!((v - 47.99).abs() < 0.01);
        assert!(parse_nvidia_smi("").is_none());
        let mi = "MemTotal:       16303428 kB\nMemFree:  1 kB\n";
        assert_eq!(parse_meminfo(mi), Some(16_303_428 * 1024));
    }

    #[test]
    fn detects_this_machine() {
        let hw = detect_hardware();
        assert!(hw.cpu_threads >= 1);
        assert!(!hw.os.is_empty() && !hw.arch.is_empty());
        if cfg!(target_os = "linux") {
            assert!(hw.ram_gb > 0.5, "{hw:?}");
        }
    }
}
