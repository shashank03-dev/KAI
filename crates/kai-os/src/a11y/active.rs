//! What the last tree read learned about the active window: its title (for
//! the `window_title` probe, `docs/VERIFICATION.md`) and, on Linux, its
//! app's accessibility bus name (to count only that app's events). Set by
//! the platform read, so it costs no extra OS call.

use std::sync::Mutex;

#[derive(Default, Clone)]
struct Active {
    title: Option<String>,
    bus: Option<String>,
}

static ACTIVE: Mutex<Active> = Mutex::new(Active {
    title: None,
    bus: None,
});

/// Record the active window seen by this read (`None` when none was).
pub(crate) fn set(title: Option<String>, bus: Option<String>) {
    if let Ok(mut a) = ACTIVE.lock() {
        a.title = title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        a.bus = bus;
    }
}

/// The active window's title from the last tree read.
pub fn title() -> Option<String> {
    ACTIVE.lock().ok().and_then(|a| a.title.clone())
}

/// The active app's accessibility bus name from the last tree read.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn bus() -> Option<String> {
    ACTIVE.lock().ok().and_then(|a| a.bus.clone())
}

#[cfg(test)]
mod tests {
    #[test]
    fn title_is_trimmed_and_empty_is_none() {
        super::set(Some("  Thank you — Firefox ".into()), Some(":1.42".into()));
        assert_eq!(super::title().as_deref(), Some("Thank you — Firefox"));
        assert_eq!(super::bus().as_deref(), Some(":1.42"));
        super::set(Some("   ".into()), None);
        assert_eq!(super::title(), None);
    }
}
