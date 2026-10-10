//! Recording runs as training data (`kai-trajectory/1`, see `docs/DATASET.md`).
//!
//! A [`TraceRecorder`] owns one run directory:
//! - `system.txt` — the system prompt, once;
//! - `frames/step-NNN.jpg` (+ `-crop.jpg`) — the exact JPEGs sent;
//! - `trace.json` — the [`Trace`], with each step's model call filled in;
//! - `meta.json` — [`RunMeta`]: task, outcome, check and failure label.
//!
//! OS-free: plain `std::fs`. The engine feeds it per model call when
//! [`crate::RunOptions::recorder`] is set; recording is off unless a caller
//! asks for it. Nothing here is uploaded anywhere.

use crate::client::Images;
use crate::failure::{self, Failure};
use crate::trace::{Outcome, Trace};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The `schema` value of every `meta.json`.
pub const SCHEMA: &str = "kai-trajectory/1";

/// `meta.json` of a recorded run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunMeta {
    pub schema: String,
    pub run_id: String,
    pub task: String,
    /// The bench task id, when the run came from `kai bench`.
    #[serde(default)]
    pub task_id: Option<String>,
    /// `bench`, `run` or `app`.
    pub source: String,
    pub backend: String,
    pub model: String,
    pub kai_version: String,
    /// `linux` or `windows`.
    pub os: String,
    /// Screen size in pixels, when known.
    #[serde(default)]
    pub screen: Option<[u32; 2]>,
    pub started: String,
    /// The bench's automatic check result.
    #[serde(default)]
    pub check: Option<bool>,
    #[serde(default)]
    pub outcome: Option<Outcome>,
    #[serde(default)]
    pub failure: Option<Failure>,
}

impl RunMeta {
    /// Meta with the given identity; the rest is filled in as the run goes.
    pub fn new(
        run_id: impl Into<String>,
        task: impl Into<String>,
        source: &str,
        backend: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            schema: SCHEMA.to_string(),
            run_id: run_id.into(),
            task: task.into(),
            task_id: None,
            source: source.to_string(),
            backend: backend.into(),
            model: model.into(),
            kai_version: env!("CARGO_PKG_VERSION").to_string(),
            os: if cfg!(windows) { "windows" } else { "linux" }.to_string(),
            screen: None,
            started: jiff::Zoned::now().to_string(),
            check: None,
            outcome: None,
            failure: None,
        }
    }
}

/// Hex FNV-1a (64-bit) of a text: the `prompt_system_hash` of each step.
pub fn fnv1a_hex(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

struct State {
    meta: RunMeta,
    /// Hash of the system prompt already written.
    system_hash: Option<String>,
    /// Model id used for steps answered by the careful client.
    careful_model: Option<String>,
}

/// Writes one recorded run. Shared by reference (`&TraceRecorder`) with the
/// engine; interior state is behind a mutex.
pub struct TraceRecorder {
    dir: PathBuf,
    state: Mutex<State>,
}

impl TraceRecorder {
    /// Create (or reuse) `dir` and its `frames/` folder.
    pub fn new(dir: impl Into<PathBuf>, meta: RunMeta) -> io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(dir.join("frames"))?;
        Ok(Self {
            dir,
            state: Mutex::new(State {
                meta,
                system_hash: None,
                careful_model: None,
            }),
        })
    }

    /// The model id to record for steps the careful client answered.
    pub fn with_careful_model(self, model: impl Into<String>) -> Self {
        self.state.lock().unwrap().careful_model = Some(model.into());
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A copy of the current meta.
    pub fn meta(&self) -> RunMeta {
        self.state.lock().unwrap().meta.clone()
    }

    /// Change the meta (e.g. set `task_id` or `kai_version`).
    pub fn update_meta(&self, f: impl FnOnce(&mut RunMeta)) {
        f(&mut self.state.lock().unwrap().meta);
    }

    /// Write `system.txt` (first call only) and return the prompt's hash.
    pub fn record_system(&self, system: &str) -> String {
        let hash = fnv1a_hex(system);
        let mut st = self.state.lock().unwrap();
        if st.system_hash.as_deref() != Some(hash.as_str()) {
            if let Err(e) = std::fs::write(self.dir.join("system.txt"), system) {
                log::warn!("recorder: could not write system.txt: {e}");
            }
            st.system_hash = Some(hash.clone());
        }
        hash
    }

    /// The model id for a step (`careful` = answered by the careful client).
    pub fn model_for(&self, careful: bool) -> Option<String> {
        let st = self.state.lock().unwrap();
        let m = if careful {
            st.careful_model
                .clone()
                .unwrap_or_else(|| st.meta.model.clone())
        } else {
            st.meta.model.clone()
        };
        (!m.is_empty()).then_some(m)
    }

    /// Note the screen size from the first observation that knows it.
    pub fn note_screen(&self, w: f32, h: f32) {
        let mut st = self.state.lock().unwrap();
        if st.meta.screen.is_none() && w > 0.0 && h > 0.0 {
            st.meta.screen = Some([w.round() as u32, h.round() as u32]);
        }
    }

    /// Write the JPEGs sent at `step`. Returns the relative paths of the
    /// screenshot (none on tree-only steps) and of the crop.
    pub fn record_frames(&self, step: u32, images: &Images) -> (Option<String>, Option<String>) {
        let write = |name: String, bytes: &[u8]| -> Option<String> {
            let rel = format!("frames/{name}");
            match std::fs::write(self.dir.join(&rel), bytes) {
                Ok(()) => Some(rel),
                Err(e) => {
                    log::warn!("recorder: could not write {rel}: {e}");
                    None
                }
            }
        };
        let shot = (!images.screenshot_jpeg.is_empty())
            .then(|| write(format!("step-{step:03}.jpg"), &images.screenshot_jpeg))
            .flatten();
        let crop = images
            .crop_jpeg
            .as_deref()
            .filter(|c| !c.is_empty())
            .and_then(|c| write(format!("step-{step:03}-crop.jpg"), c));
        (shot, crop)
    }

    /// Write `trace.json` and `meta.json` (outcome + failure label with the
    /// check known so far). Called by the engine when the run ends.
    pub fn finish(&self, trace: &Trace) -> io::Result<()> {
        std::fs::write(
            self.dir.join("trace.json"),
            serde_json::to_string_pretty(trace)? + "\n",
        )?;
        let check = self.state.lock().unwrap().meta.check;
        self.set_verdict(trace, check, None)
    }

    /// Record the automatic check (bench) and relabel the failure; `failure`
    /// overrides the heuristic label (e.g. "setup failed"). Rewrites
    /// `meta.json`.
    pub fn set_verdict(
        &self,
        trace: &Trace,
        check: Option<bool>,
        failure: Option<Failure>,
    ) -> io::Result<()> {
        let mut st = self.state.lock().unwrap();
        st.meta.check = check;
        st.meta.outcome = trace.outcome;
        if st.meta.started.is_empty() {
            st.meta.started = trace.started.clone();
        }
        st.meta.failure = if failure::passed(trace, check) {
            None
        } else {
            failure.or_else(|| failure::classify(trace, check))
        };
        std::fs::write(
            self.dir.join("meta.json"),
            serde_json::to_string_pretty(&st.meta)? + "\n",
        )
    }
}

/// A run directory found under a dataset root.
#[derive(Debug, Clone)]
pub struct RecordedRun {
    pub dir: PathBuf,
    pub meta: RunMeta,
    pub trace: Trace,
    /// `system.txt`, empty if missing.
    pub system: String,
}

/// Load every run under `root` (any depth): directories that hold both
/// `meta.json` and `trace.json`. Sorted by path; unreadable runs are skipped
/// with a warning.
pub fn load_runs(root: &Path) -> io::Result<Vec<RecordedRun>> {
    let mut dirs = Vec::new();
    find_runs(root, &mut dirs, 0)?;
    dirs.sort();
    let mut out = Vec::new();
    for dir in dirs {
        match load_run(&dir) {
            Ok(r) => out.push(r),
            Err(e) => log::warn!("skipping {}: {e}", dir.display()),
        }
    }
    Ok(out)
}

fn find_runs(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) -> io::Result<()> {
    if dir.join("meta.json").is_file() && dir.join("trace.json").is_file() {
        out.push(dir.to_path_buf());
        return Ok(());
    }
    if depth > 6 {
        return Ok(());
    }
    for e in std::fs::read_dir(dir)?.flatten() {
        let p = e.path();
        if p.is_dir() && p.file_name().is_some_and(|n| n != "frames") {
            find_runs(&p, out, depth + 1)?;
        }
    }
    Ok(())
}

/// Load one run directory.
pub fn load_run(dir: &Path) -> anyhow::Result<RecordedRun> {
    let meta: RunMeta = serde_json::from_str(&std::fs::read_to_string(dir.join("meta.json"))?)?;
    let trace: Trace = serde_json::from_str(&std::fs::read_to_string(dir.join("trace.json"))?)?;
    let system = std::fs::read_to_string(dir.join("system.txt")).unwrap_or_default();
    Ok(RecordedRun {
        dir: dir.to_path_buf(),
        meta,
        trace,
        system,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{StepRecord, Timings};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kai-rec-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn writes_frames_system_trace_and_meta() {
        let root = tmp("write");
        let dir = root.join("batch").join("t1");
        let rec = TraceRecorder::new(&dir, RunMeta::new("batch/t1", "do x", "bench", "mock", "m"))
            .unwrap()
            .with_careful_model("big");
        let h = rec.record_system("SYSTEM");
        assert_eq!(h, fnv1a_hex("SYSTEM"));
        assert_eq!(h.len(), 16);
        let imgs = Images {
            screenshot_jpeg: vec![0xff, 0xd8, 0xff, 0xd9],
            crop_jpeg: Some(vec![1, 2, 3]),
        };
        let (s, c) = rec.record_frames(7, &imgs);
        assert_eq!(s.as_deref(), Some("frames/step-007.jpg"));
        assert_eq!(c.as_deref(), Some("frames/step-007-crop.jpg"));
        assert!(dir.join("frames/step-007-crop.jpg").is_file());
        let tree_only = Images {
            screenshot_jpeg: vec![],
            crop_jpeg: None,
        };
        assert_eq!(rec.record_frames(8, &tree_only), (None, None));
        assert_eq!(rec.model_for(true).as_deref(), Some("big"));
        assert_eq!(rec.model_for(false).as_deref(), Some("m"));
        rec.note_screen(1280.0, 800.0);

        let mut t = Trace::new("do x");
        let d = crate::Decision::parse_checked(r#"{"thought":"","action":"done","risk":"none"}"#)
            .unwrap();
        let mut step = StepRecord::new(0, &d, Timings::default(), "Mock");
        step.call.reply_raw = Some("{}".into());
        t.push(step);
        t.finish(Outcome::Success);
        rec.finish(&t).unwrap();
        rec.set_verdict(&t, Some(false), None).unwrap();

        let runs = load_runs(&root).unwrap();
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!(r.system, "SYSTEM");
        assert_eq!(r.meta.schema, SCHEMA);
        assert_eq!(r.meta.screen, Some([1280, 800]));
        assert_eq!(r.meta.check, Some(false));
        assert_eq!(
            r.meta.failure.as_ref().map(|f| f.category),
            Some(failure::Category::FalseDone)
        );
        assert_eq!(r.trace.steps[0].call.reply_raw.as_deref(), Some("{}"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn step_call_fields_are_flat_and_optional() {
        let d = crate::Decision::parse_checked(r#"{"thought":"","action":"done","risk":"none"}"#)
            .unwrap();
        let mut s = StepRecord::new(0, &d, Timings::default(), "Mock");
        let plain = serde_json::to_value(&s).unwrap();
        assert!(plain.get("reply_raw").is_none() && plain.get("call").is_none());
        s.call.prompt_user = Some("u".into());
        s.call.prompt_tokens = Some(5);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["prompt_user"], "u");
        assert_eq!(v["prompt_tokens"], 5);
        let back: StepRecord = serde_json::from_value(v).unwrap();
        assert_eq!(back.call.prompt_user.as_deref(), Some("u"));
    }
}
