/// One clipboard handle for the whole process. On X11 the clipboard only
/// holds what we set while the handle that set it is alive (arboard serves
/// paste requests from it); a handle dropped right after `set_text` left the
/// target app pasting nothing.
static CLIPBOARD: std::sync::LazyLock<std::sync::Mutex<Option<arboard::Clipboard>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(arboard::Clipboard::new().ok()));

pub(super) fn clipboard_get() -> Option<String> {
    CLIPBOARD.lock().ok()?.as_mut()?.get_text().ok()
}

pub(super) fn clipboard_set(text: &str) -> anyhow::Result<()> {
    let mut guard = CLIPBOARD
        .lock()
        .map_err(|_| anyhow::anyhow!("clipboard lock poisoned"))?;
    let cb = guard
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("no clipboard available"))?;
    cb.set_text(text.to_string())?;
    Ok(())
}
