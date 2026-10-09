//! One run's controls, shared between the UI and the engine: stop, pause and
//! the steering queue. The engine asks `Interrupt::stopped()` before every
//! step (and the orchestrator before every wave of sub-agents), so pausing is
//! done there: while paused, `stopped()` waits, so the engine holds still
//! *between* steps and never in the middle of an action. Steering messages
//! wait in a queue the engine drains once per step (`Steering::take`).

use futures::channel::mpsc::UnboundedSender;
use kai_core::engine::{Interrupt, Steering};
use kai_os::KillSwitch;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How often a paused run looks at its flags.
const PAUSE_POLL: Duration = Duration::from_millis(100);

pub struct RunControl {
    /// The app-wide kill switch (hotkey, top-left corner, stop button).
    kill: KillSwitch,
    /// Stop this run only.
    stop: AtomicBool,
    paused: AtomicBool,
    steer: Mutex<Vec<String>>,
    /// Told when the engine actually holds at a pause (true) or goes on (false).
    held: Mutex<Option<UnboundedSender<bool>>>,
}

impl RunControl {
    pub fn new(kill: KillSwitch) -> Self {
        Self {
            kill,
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            steer: Mutex::new(Vec::new()),
            held: Mutex::new(None),
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::SeqCst) || self.kill.is_tripped()
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Queue a message for the agent's next step.
    pub fn steer(&self, message: String) {
        if let Ok(mut q) = self.steer.lock() {
            q.push(message);
        }
    }

    /// Where to report that a pause took hold.
    pub fn report_to(&self, tx: UnboundedSender<bool>) {
        if let Ok(mut h) = self.held.lock() {
            *h = Some(tx);
        }
    }

    /// Stop reporting (the run is over).
    pub fn detach_reporter(&self) {
        if let Ok(mut h) = self.held.lock() {
            *h = None;
        }
    }

    fn tell(&self, held: bool) {
        if let Ok(h) = self.held.lock()
            && let Some(tx) = h.as_ref()
        {
            let _ = tx.unbounded_send(held);
        }
    }

    /// Block until resumed or stopped.
    fn hold(&self) {
        while self.is_paused() && !self.stop_requested() {
            std::thread::sleep(PAUSE_POLL);
        }
    }
}

impl Interrupt for RunControl {
    fn stopped(&self) -> bool {
        if self.is_paused() && !self.stop_requested() {
            self.tell(true);
            // The engine calls this from async code on the agent's
            // multi-threaded runtime: `block_in_place` lets the runtime move
            // other work off this thread while the run holds still.
            match tokio::runtime::Handle::try_current() {
                Ok(h) if h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
                    tokio::task::block_in_place(|| self.hold())
                }
                _ => self.hold(),
            }
            self.tell(false);
        }
        self.stop_requested()
    }
}

impl Steering for RunControl {
    fn take(&self) -> Vec<String> {
        self.steer
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn steering_drains_once() {
        let c = RunControl::new(KillSwitch::new());
        c.steer("use Firefox".into());
        c.steer("not Chrome".into());
        assert_eq!(c.take(), vec!["use Firefox", "not Chrome"]);
        assert!(c.take().is_empty());
    }

    #[test]
    fn stop_and_kill_both_stop() {
        let kill = KillSwitch::new();
        let c = RunControl::new(kill.clone());
        assert!(!c.stopped());
        c.stop();
        assert!(c.stopped());
        let d = RunControl::new(kill.clone());
        kill.trip("test");
        assert!(d.stopped());
    }

    #[test]
    fn pause_holds_until_resumed() {
        let c = Arc::new(RunControl::new(KillSwitch::new()));
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        c.report_to(tx);
        c.set_paused(true);
        let other = c.clone();
        let resumer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            other.set_paused(false);
        });
        let started = Instant::now();
        assert!(!c.stopped(), "resumed, not stopped");
        assert!(started.elapsed() >= Duration::from_millis(200));
        resumer.join().unwrap();
        assert!(rx.try_recv().unwrap());
        assert!(!rx.try_recv().unwrap());
    }

    #[test]
    fn stopping_a_paused_run_releases_it() {
        let c = Arc::new(RunControl::new(KillSwitch::new()));
        c.set_paused(true);
        let other = c.clone();
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            other.stop();
        });
        assert!(c.stopped());
        stopper.join().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pause_inside_the_runtime_does_not_starve_it() {
        let c = Arc::new(RunControl::new(KillSwitch::new()));
        c.set_paused(true);
        let held = {
            let c = c.clone();
            tokio::spawn(async move { c.stopped() })
        };
        // Other tasks still run while the run is paused.
        let other = tokio::spawn(async { 7 });
        assert_eq!(other.await.unwrap(), 7);
        c.set_paused(false);
        assert!(!held.await.unwrap());
    }
}
