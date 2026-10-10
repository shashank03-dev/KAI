//! Free disk space, checked before any download starts.

use std::path::Path;

/// Bytes free for this user on the volume holding `path` (which must exist).
pub fn free_bytes(path: &Path) -> anyhow::Result<u64> {
    imp(path)
}

/// Fail with a plain message when `needed` bytes (plus a 1 GB margin) do not
/// fit in `free`.
pub fn check_space(needed: u64, free: u64, where_: &Path) -> anyhow::Result<()> {
    const MARGIN: u64 = 1_000_000_000;
    let gb = |b: u64| b as f64 / 1e9;
    anyhow::ensure!(
        free >= needed.saturating_add(MARGIN),
        "not enough disk space: this needs {:.1} GB (plus 1 GB to spare) but only {:.1} GB \
         is free at {}",
        gb(needed),
        gb(free),
        where_.display()
    );
    Ok(())
}

#[cfg(unix)]
fn imp(path: &Path) -> anyhow::Result<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `s` a writable statvfs.
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut s) };
    anyhow::ensure!(
        rc == 0,
        "cannot read free space of {}: {}",
        path.display(),
        std::io::Error::last_os_error()
    );
    Ok(s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(windows)]
fn imp(path: &Path) -> anyhow::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut avail = 0u64;
    // SAFETY: `wide` is NUL-terminated; the out pointer is a valid u64.
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut avail), None, None) }?;
    Ok(avail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_check() {
        let p = Path::new("/x");
        assert!(check_space(1_000, 5_000_000_000, p).is_ok());
        let e = check_space(15_000_000_000, 10_000_000_000, p).unwrap_err();
        assert!(e.to_string().contains("needs 15.0 GB"), "{e}");
        assert!(free_bytes(&std::env::temp_dir()).unwrap() > 0);
    }
}
