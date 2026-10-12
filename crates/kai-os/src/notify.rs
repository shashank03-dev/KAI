//! A desktop notification for the moment Kai needs the person (a CAPTCHA, a
//! one-time code, a question; `docs/HUMAN_CHECKS.md`). Best effort: Linux
//! goes through the freedesktop notification service on the session bus;
//! elsewhere this returns `false` and the app's always-on-top mini window
//! (or the terminal bell) is the signal. Never carries a code or an answer:
//! only Kai's own title and prompt.

/// Never wait longer than this: with no notification daemon a D-Bus call
/// can hang until its own ~25 s timeout, and the person must not wait on it.
const LIMIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Show a notification. `true` when the desktop accepted it within 2 s.
pub async fn desktop(title: &str, body: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        match tokio::time::timeout(LIMIT, linux::notify(title, body)).await {
            Ok(Ok(())) => true,
            Ok(Err(e)) => {
                log::info!("no desktop notification: {e}");
                false
            }
            Err(_) => {
                log::info!("no desktop notification: no answer within {LIMIT:?}");
                false
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (title, body, LIMIT);
        false
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use zbus::zvariant::Value;

    /// `org.freedesktop.Notifications.Notify`, urgency "critical" so it
    /// stays until the person sees it.
    pub async fn notify(title: &str, body: &str) -> zbus::Result<()> {
        let conn = zbus::Connection::session().await?;
        let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
        hints.insert("urgency", Value::U8(2));
        conn.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "Kai",
                0u32,
                "dialog-information",
                title,
                body,
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn never_blocks_the_person_for_long() {
        // With or without a session bus or notification daemon (CI has
        // neither), the call returns within its bound.
        let t = std::time::Instant::now();
        let _ = super::desktop("Kai test", "notification smoke test").await;
        assert!(t.elapsed() < std::time::Duration::from_secs(3));
    }
}
