//! A cancelled CDP await still releases held input on the original session.
//! Unacknowledged cleanup poisons the lane until the browser is restarted.
//! A call that returned an error (a dialog opened, the tab closed) is not a
//! cancellation: the release is sent and the lane stays usable.
use super::cdp::Cdp;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
pub(super) trait Cleanup: Sync {
    fn send(&self, method: &str, params: Value, session: &str);
}
impl Cleanup for Cdp {
    fn send(&self, method: &str, params: Value, session: &str) {
        let _ = self.send_cleanup(method, params, session);
    }
}
pub(super) struct Release<'a> {
    sink: &'a dyn Cleanup,
    poisoned: &'a AtomicBool,
    method: &'static str,
    params: Value,
    session: String,
    armed: bool,
}
impl<'a> Release<'a> {
    pub fn new(
        sink: &'a dyn Cleanup,
        poisoned: &'a AtomicBool,
        method: &'static str,
        params: Value,
        session: String,
    ) -> Self {
        Self {
            sink,
            poisoned,
            method,
            params,
            session,
            armed: true,
        }
    }
    pub fn disarm(&mut self) {
        self.armed = false;
    }
    /// The press or release came back with an error: send the release
    /// anyway (best effort), without poisoning the lane.
    pub fn settle(mut self) {
        self.armed = false;
        self.sink
            .send(self.method, self.params.clone(), &self.session);
    }
}
/// Sends input with `release` armed: disarmed once acknowledged, settled
/// on an error, and left armed (poisoning) only if the await is cancelled.
pub(super) async fn guarded<T>(
    mut release: Release<'_>,
    send: impl std::future::Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    match send.await {
        Ok(v) => {
            release.disarm();
            Ok(v)
        }
        Err(e) => {
            release.settle();
            Err(e)
        }
    }
}
impl Drop for Release<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.poisoned.store(true, Ordering::SeqCst);
            self.sink
                .send(self.method, self.params.clone(), &self.session);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Sink(std::sync::Mutex<Vec<(String, Value, String)>>);
    impl Cleanup for Sink {
        fn send(&self, m: &str, p: Value, s: &str) {
            self.0.lock().unwrap().push((m.into(), p, s.into()));
        }
    }
    #[test]
    fn dropped_input_releases_original_session_and_blocks_further_input() {
        let sink = Sink::default();
        let poison = AtomicBool::new(false);
        {
            let _held = Release::new(
                &sink,
                &poison,
                "Input.dispatchMouseEvent",
                serde_json::json!({"type":"mouseReleased","button":"left","x":1,"y":2}),
                "original".into(),
            );
        }
        assert!(poison.load(Ordering::SeqCst));
        let calls = sink.0.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, "original");
        assert_eq!(calls[0].1["type"], "mouseReleased");
    }
    #[test]
    fn an_error_releases_without_poisoning_the_lane() {
        let sink = Sink::default();
        let poison = AtomicBool::new(false);
        Release::new(
            &sink,
            &poison,
            "Input.dispatchMouseEvent",
            serde_json::json!({"type":"mouseReleased"}),
            "page".into(),
        )
        .settle();
        assert!(
            !poison.load(Ordering::SeqCst),
            "a dialog must not kill the lane"
        );
        assert_eq!(sink.0.lock().unwrap().len(), 1);
    }
    #[test]
    fn acknowledged_release_needs_no_cleanup_or_poison() {
        let sink = Sink::default();
        let poison = AtomicBool::new(false);
        {
            let mut held = Release::new(
                &sink,
                &poison,
                "Input.dispatchKeyEvent",
                serde_json::json!({"type":"keyUp"}),
                "page".into(),
            );
            held.disarm();
        }
        assert!(!poison.load(Ordering::SeqCst));
        assert!(sink.0.lock().unwrap().is_empty());
    }
}
