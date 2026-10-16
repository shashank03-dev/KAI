//! Embeds assets/icons/Kai.ico into the Windows executable, so Explorer and
//! the taskbar show the app icon. No-op on other hosts.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/Kai.ico");
    embed_icon();
}

#[cfg(windows)]
fn embed_icon() {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/icons/Kai.ico");
    if let Err(e) = res.compile() {
        // Without an rc/windres toolchain the icon is left out; the app runs.
        println!("cargo:warning=could not embed assets/icons/Kai.ico: {e}");
    }
}

#[cfg(not(windows))]
fn embed_icon() {}
