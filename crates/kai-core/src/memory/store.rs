//! The lesson store: `<dir>/lessons.jsonl`, one JSON record per line. Every
//! change appends the lesson's new state (the last line for an id wins) or a
//! `{"forget":"<id>"}` tombstone; the file is rewritten (compacted) when it
//! holds more than twice the live lessons. Local only.

use super::recall::{self, Context};
use super::scrub::{allowed, scrub};
use super::{Kind, Lesson, MERGE_SIMILARITY, Scope, Source, learn, now, similarity};
use crate::failure;
use crate::trace::Trace;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Lessons kept per app or site at most.
pub const MAX_PER_SCOPE: usize = 30;
/// Lessons kept in all at most.
pub const MAX_TOTAL: usize = 500;
/// A lesson nobody used for this long is dropped (the user's own are kept).
pub const FORGET_AFTER_DAYS: i64 = 365;
/// File name inside the memory folder.
pub const FILE: &str = "lessons.jsonl";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Forget {
    forget: String,
}

/// What [`MemoryStore::after_run`] did.
#[derive(Debug, Clone, Default)]
pub struct AfterRun {
    /// Lessons added or merged from this run, as stored.
    pub learned: Vec<Lesson>,
    /// Ids of the lessons shown this run whose counters changed.
    pub credited: Vec<String>,
    /// Lessons dropped by the caps or retired.
    pub removed: usize,
}

struct Inner {
    lessons: Vec<Lesson>,
    /// Lines in the file now.
    lines: usize,
}

/// Lessons on disk, loaded once; safe to share between runs (`&self` API).
pub struct MemoryStore {
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
}

impl MemoryStore {
    /// Load `<dir>/lessons.jsonl` (the folder is created if missing).
    /// Unreadable lines are skipped.
    pub fn load(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let path = dir.join(FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let mut lessons: Vec<Lesson> = Vec::new();
        let mut lines = 0;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            lines += 1;
            if let Ok(f) = serde_json::from_str::<Forget>(line) {
                lessons.retain(|l| l.id != f.forget);
            } else if let Ok(l) = serde_json::from_str::<Lesson>(line) {
                match lessons.iter_mut().find(|x| x.id == l.id) {
                    Some(x) => *x = l,
                    None => lessons.push(l),
                }
            }
        }
        let store = Self {
            path: Some(path),
            inner: Mutex::new(Inner { lessons, lines }),
        };
        let removed = store.lock().enforce(now());
        if !removed.is_empty() || store.lock().needs_compaction() {
            store.compact()?;
        }
        Ok(store)
    }

    /// A store that is never written (bench A/B runs, tests).
    pub fn in_memory() -> Self {
        Self {
            path: None,
            inner: Mutex::new(Inner {
                lessons: Vec::new(),
                lines: 0,
            }),
        }
    }

    /// The JSONL file, when on disk.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Every lesson, most confident first.
    pub fn lessons(&self) -> Vec<Lesson> {
        let mut v = self.lock().lessons.clone();
        v.sort_by(|a, b| {
            b.confidence()
                .total_cmp(&a.confidence())
                .then(b.touched().cmp(&a.touched()))
        });
        v
    }

    pub fn len(&self) -> usize {
        self.lock().lessons.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every app some lesson is scoped to (to spot them on screen).
    pub fn apps(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for l in &self.lock().lessons {
            for a in &l.scope.apps {
                if !v.contains(a) {
                    v.push(a.clone());
                }
            }
        }
        v
    }

    /// The lessons worth showing for this task and the apps/sites in play.
    pub fn recall(&self, task: &str, apps: &[String], domains: &[String]) -> Vec<Lesson> {
        self.recall_in(&Context::new(task, apps, domains))
    }

    /// [`MemoryStore::recall`] for a prepared context.
    pub fn recall_in(&self, cx: &Context) -> Vec<Lesson> {
        let g = self.lock();
        recall::rank(&g.lessons, cx, now())
            .into_iter()
            .cloned()
            .collect()
    }

    /// Add a lesson, merging it into a near-duplicate of the same kind.
    /// Returns the lesson as stored.
    pub fn add(&self, lesson: Lesson) -> std::io::Result<Lesson> {
        let mut g = self.lock();
        let stored = g.insert(lesson);
        let removed = g.enforce(now());
        let kept = g.lessons.iter().find(|l| l.id == stored.id).cloned();
        drop(g);
        self.append(kept.as_slice(), &removed)?;
        Ok(kept.unwrap_or(stored))
    }

    /// A lesson the user typed. Refused when empty, longer than
    /// [`super::MAX_WORDS`] words, or carrying something secret-looking.
    pub fn add_user(&self, kind: Kind, text: &str, scope: Scope) -> anyhow::Result<Lesson> {
        let text = text.trim();
        anyhow::ensure!(!text.is_empty(), "the lesson is empty");
        let words = text.split_whitespace().count();
        anyhow::ensure!(
            words <= super::MAX_WORDS,
            "a lesson is at most {} words (this one has {words})",
            super::MAX_WORDS
        );
        anyhow::ensure!(
            allowed(&scrub(text), &[], true) && scrub(text) == text,
            "the lesson looks like it holds an email, a key or a long number; not stored"
        );
        Ok(self.add(Lesson::new(kind, text, scope, Source::User))?)
    }

    /// Delete a lesson by id (or a unique id prefix). False when none matched.
    pub fn forget(&self, id: &str) -> std::io::Result<bool> {
        let id = id.trim();
        let mut g = self.lock();
        let hits: Vec<String> = g
            .lessons
            .iter()
            .filter(|l| !id.is_empty() && l.id.starts_with(id))
            .map(|l| l.id.clone())
            .collect();
        if hits.len() != 1 {
            return Ok(false);
        }
        g.lessons.retain(|l| l.id != hits[0]);
        drop(g);
        self.append(&[], &hits)?;
        Ok(true)
    }

    /// Delete every lesson (the file is emptied).
    pub fn clear(&self) -> std::io::Result<()> {
        self.lock().lessons.clear();
        self.compact()
    }

    /// After a run: credit the lessons it was shown (`trace.lessons`), then
    /// learn from it. `extra`: lessons from the reflection call. With
    /// `learn` off nothing changes.
    pub fn after_run(
        &self,
        trace: &Trace,
        check: Option<bool>,
        extra: Vec<Lesson>,
        learn_on: bool,
    ) -> std::io::Result<AfterRun> {
        let mut out = AfterRun::default();
        if !learn_on {
            return Ok(out);
        }
        let passed = failure::passed(trace, check);
        let category = failure::classify(trace, check).map(|f| f.category);
        let t = now();
        let mut changed: Vec<String> = Vec::new();
        let mut g = self.lock();
        for id in &trace.lessons {
            let Some(l) = g.lessons.iter_mut().find(|l| &l.id == id) else {
                continue;
            };
            l.uses += 1;
            l.last_used = t;
            match (passed, l.targets, category) {
                (true, _, _) => l.helped += 1,
                (false, Some(tg), Some(c)) if tg == c => l.hurt += 1,
                (false, Some(_), Some(_)) => l.helped += 1,
                _ => {}
            }
            out.credited.push(l.id.clone());
            changed.push(l.id.clone());
        }
        for l in learn::learn(trace, check).into_iter().chain(extra) {
            let s = g.insert(l);
            changed.push(s.id.clone());
            out.learned.push(s);
        }
        let removed = g.enforce(t);
        out.removed = removed.len();
        let rows: Vec<Lesson> = g
            .lessons
            .iter()
            .filter(|l| changed.contains(&l.id))
            .cloned()
            .collect();
        // Report the lessons as stored (merged counters included).
        for l in out.learned.iter_mut() {
            if let Some(s) = rows.iter().find(|s| s.id == l.id) {
                *l = s.clone();
            }
        }
        drop(g);
        self.append(&rows, &removed)?;
        Ok(out)
    }

    /// Append records; compact when the file has grown too much.
    fn append(&self, rows: &[Lesson], forgotten: &[String]) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if rows.is_empty() && forgotten.is_empty() {
            return Ok(());
        }
        let mut buf = String::new();
        for l in rows {
            buf.push_str(&serde_json::to_string(l).map_err(std::io::Error::other)?);
            buf.push('\n');
        }
        for id in forgotten {
            let f = Forget { forget: id.clone() };
            buf.push_str(&serde_json::to_string(&f).map_err(std::io::Error::other)?);
            buf.push('\n');
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        f.write_all(buf.as_bytes())?;
        let compact = {
            let mut g = self.lock();
            g.lines += rows.len() + forgotten.len();
            g.needs_compaction()
        };
        if compact { self.compact() } else { Ok(()) }
    }

    /// Rewrite the file with one line per live lesson.
    pub fn compact(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let mut g = self.lock();
        let mut buf = String::new();
        for l in &g.lessons {
            buf.push_str(&serde_json::to_string(l).map_err(std::io::Error::other)?);
            buf.push('\n');
        }
        let tmp = path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, buf)?;
        std::fs::rename(&tmp, path)?;
        g.lines = g.lessons.len();
        Ok(())
    }
}

impl Inner {
    fn needs_compaction(&self) -> bool {
        self.lines > 2 * self.lessons.len() + 64
    }

    /// Merge into a near-duplicate of the same kind, or add.
    fn insert(&mut self, l: Lesson) -> Lesson {
        let dup = self
            .lessons
            .iter_mut()
            .filter(|x| x.kind == l.kind)
            .map(|x| {
                let s = similarity(&x.text, &l.text);
                (x, s)
            })
            .filter(|(_, s)| *s >= MERGE_SIMILARITY)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(x, _)| x);
        match dup {
            Some(x) => {
                for e in &l.evidence {
                    if !x.evidence.contains(e) && x.evidence.len() < 20 {
                        x.evidence.push(e.clone());
                    }
                }
                x.scope.merge(&l.scope);
                x.uses += l.uses;
                x.helped += l.helped;
                x.hurt += l.hurt;
                x.last_used = x.last_used.max(l.last_used);
                if l.source == Source::User {
                    x.source = Source::User;
                }
                if x.targets.is_none() {
                    x.targets = l.targets;
                }
                x.clone()
            }
            None => {
                self.lessons.push(l.clone());
                l
            }
        }
    }

    /// Retire, age out and cap. Returns the ids removed.
    fn enforce(&mut self, now: i64) -> Vec<String> {
        let mut removed = Vec::new();
        let old = now - FORGET_AFTER_DAYS * 86_400;
        self.lessons.retain(|l| {
            let gone = l.retired() || (l.source != Source::User && l.touched() < old);
            if gone {
                removed.push(l.id.clone());
            }
            !gone
        });
        // Eviction order: not the user's own first, then lowest confidence,
        // then least recently touched.
        let order = |a: &Lesson, b: &Lesson| {
            (a.source == Source::User)
                .cmp(&(b.source == Source::User))
                .then(a.confidence().total_cmp(&b.confidence()))
                .then(a.touched().cmp(&b.touched()))
        };
        let mut keys: Vec<String> = Vec::new();
        for l in &self.lessons {
            for k in l.scope.apps.iter().chain(&l.scope.domains) {
                if !keys.contains(k) {
                    keys.push(k.clone());
                }
            }
        }
        for k in keys {
            let mut inside: Vec<&Lesson> = self
                .lessons
                .iter()
                .filter(|l| l.scope.apps.contains(&k) || l.scope.domains.contains(&k))
                .collect();
            if inside.len() <= MAX_PER_SCOPE {
                continue;
            }
            inside.sort_by(|a, b| order(a, b));
            let drop: Vec<String> = inside[..inside.len() - MAX_PER_SCOPE]
                .iter()
                .map(|l| l.id.clone())
                .collect();
            self.lessons.retain(|l| !drop.contains(&l.id));
            removed.extend(drop);
        }
        if self.lessons.len() > MAX_TOTAL {
            let mut all: Vec<&Lesson> = self.lessons.iter().collect();
            all.sort_by(|a, b| order(a, b));
            let drop: Vec<String> = all[..all.len() - MAX_TOTAL]
                .iter()
                .map(|l| l.id.clone())
                .collect();
            self.lessons.retain(|l| !drop.contains(&l.id));
            removed.extend(drop);
        }
        removed
    }
}
