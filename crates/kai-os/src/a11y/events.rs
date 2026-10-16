//! Linux: a running count of AT-SPI events from the active app, for the
//! per-action effect label (`docs/VERIFICATION.md`, stage 2). Counted:
//! object state, text, property, children, selection and active-descendant
//! changes, window events and document loads. Not counted: bounds, caret
//! and visible-data events, which fire on their own (spinners, blinking
//! carets). Events from any other app (Kai's own windows included) are
//! ignored. Until the listener is running, [`count`] is `None` and the
//! engine falls back to the tree diff and screen hash.

use atspi::events::{DocumentEvents, ObjectEvents, WindowEvents};
use futures::StreamExt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

static COUNT: AtomicU64 = AtomicU64::new(0);
static LISTENING: AtomicBool = AtomicBool::new(false);
/// A listener task exists (running, or waiting to allow a restart).
static STARTED: AtomicBool = AtomicBool::new(false);
/// After the listener fails or its stream ends (the bus restarted), the
/// next look starts a new one no sooner than this.
const RESTART_AFTER: Duration = Duration::from_secs(30);

/// Object-event members that fire by themselves and say nothing about an
/// action.
const NOISY: &[&str] = &["BoundsChanged", "VisibleDataChanged", "TextCaretMoved"];

/// Start the listener on the current Tokio runtime unless one is running.
/// Cheap to call on every observation; off a runtime it does nothing (and a
/// later call on one still starts it).
pub fn start() {
    let Ok(rt) = tokio::runtime::Handle::try_current() else {
        return;
    };
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    rt.spawn(async {
        match listen().await {
            Ok(()) => log::warn!("accessibility event stream ended; restarting it later"),
            Err(e) => log::warn!("accessibility events unavailable: {e}"),
        }
        LISTENING.store(false, Ordering::SeqCst);
        tokio::time::sleep(RESTART_AFTER).await;
        STARTED.store(false, Ordering::SeqCst);
    });
}

async fn listen() -> anyhow::Result<()> {
    let a11y = super::linux::conn().await?;
    a11y.register_event::<ObjectEvents>().await?;
    a11y.register_event::<WindowEvents>().await?;
    a11y.register_event::<DocumentEvents>().await?;
    let mut stream = zbus::MessageStream::from(a11y.connection());
    LISTENING.store(true, Ordering::SeqCst);
    while let Some(msg) = stream.next().await {
        let Ok(msg) = msg else { continue };
        if msg.message_type() != zbus::message::Type::Signal {
            continue;
        }
        let hdr = msg.header();
        let (Some(iface), Some(member), Some(sender)) =
            (hdr.interface(), hdr.member(), hdr.sender())
        else {
            continue;
        };
        if counts(iface.as_str(), member.as_str())
            && super::active::bus().as_deref() == Some(sender.as_str())
        {
            COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }
    Ok(())
}

/// Whether a signal on `iface`/`member` says the app changed something.
fn counts(iface: &str, member: &str) -> bool {
    match iface {
        "org.a11y.atspi.Event.Object" => !NOISY.contains(&member),
        "org.a11y.atspi.Event.Window" | "org.a11y.atspi.Event.Document" => true,
        _ => false,
    }
}

/// Events counted so far; `None` while the listener is not running.
pub fn count() -> Option<u64> {
    LISTENING
        .load(Ordering::SeqCst)
        .then(|| COUNT.load(Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_meaningful_events_count() {
        assert!(super::counts("org.a11y.atspi.Event.Object", "StateChanged"));
        assert!(super::counts("org.a11y.atspi.Event.Object", "TextChanged"));
        assert!(super::counts(
            "org.a11y.atspi.Event.Document",
            "LoadComplete"
        ));
        assert!(!super::counts(
            "org.a11y.atspi.Event.Object",
            "BoundsChanged"
        ));
        assert!(!super::counts(
            "org.a11y.atspi.Event.Object",
            "TextCaretMoved"
        ));
        assert!(!super::counts("org.a11y.atspi.Event.Mouse", "Button"));
        assert_eq!(super::count(), None, "not listening in tests");
    }

    #[test]
    fn a_first_call_off_the_runtime_does_not_block_a_later_start() {
        std::thread::spawn(super::start).join().unwrap();
        assert!(!super::STARTED.load(super::Ordering::SeqCst));
    }
}
