//! Windows: ask the OS to leave a window out of every screen capture, so Kai's
//! own windows never reach the model. Linux has no such call; there the app
//! reports its window rectangles and the perceptor paints over them.

/// `hwnd` as the raw handle value.
#[cfg(target_os = "windows")]
pub fn from_capture(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
    };
    // SAFETY: a plain Win32 call on a window handle the caller owns.
    if let Err(e) =
        unsafe { SetWindowDisplayAffinity(HWND(hwnd as *mut _), WDA_EXCLUDEFROMCAPTURE) }
    {
        log::warn!("could not exclude window from capture: {e}");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn from_capture(_hwnd: isize) {}
