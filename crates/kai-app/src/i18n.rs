//! The app chrome's strings: navigation, the home headline, the run and the
//! result. The UI is English-only (DECISIONS.md), so page copy stays inline
//! next to the layout it belongs to; this table is the seam a locale would
//! start from. Ely's own messages read its English builtin catalog (no
//! `I18n` global is installed).

use gpui::App;

const EN: &[(&str, &str)] = &[
    ("app.name", "Kai"),
    ("app.tagline", "Gemma 4's hands on your computer"),
    ("splash.waking", "Waking Gemma…"),
    ("nav.run", "Run"),
    ("nav.models", "Models"),
    ("nav.skills", "Skills"),
    ("nav.history", "History"),
    ("nav.bench", "Benchmarks"),
    ("nav.settings", "Settings"),
    ("home.ask", "What should Kai do on your computer?"),
    (
        "home.placeholder",
        "Describe a task, e.g. \"rename the photos in Downloads by date\"",
    ),
    ("home.team", "Team"),
    ("run.step", "Step {n} of {max}"),
    ("run.thinking", "Gemma is deciding…"),
    ("done.success", "Done"),
    ("done.failure", "Kai couldn't finish"),
    ("done.stopped", "Stopped as you asked"),
    ("done.save_skill", "Save as skill"),
    ("done.again", "Run again"),
    ("done.trace", "View trace"),
];

/// The string for `key`, with `{name}` filled.
pub fn t(_: &App, key: &str, args: &[(&str, &str)]) -> String {
    let mut s = EN
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| {
            log::error!("strings: no {key:?}");
            key.to_string()
        });
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = super::EN.iter().map(|(k, _)| *k).collect();
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), n);
    }

    /// The app's source except this file, and every key it passes to `t`
    /// as a literal, however the call is wrapped.
    fn sources(dir: &Path, all: &mut String, keys: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                sources(&p, all, keys);
            } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("i18n.rs") {
                let src = std::fs::read_to_string(&p).unwrap();
                for (at, _) in src.match_indices("t(") {
                    let before = src[..at].chars().next_back();
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let rest = src[at + 2..].trim_start();
                    let Some(rest) = rest.strip_prefix("cx") else {
                        continue;
                    };
                    let Some(rest) = rest.trim_start().strip_prefix(',') else {
                        continue;
                    };
                    if let Some(lit) = rest.trim_start().strip_prefix('"') {
                        keys.push(lit[..lit.find('"').unwrap()].to_string());
                    }
                }
                all.push_str(&src);
            }
        }
    }

    #[test]
    fn every_key_in_use_is_in_the_table_and_none_is_dead() {
        // A missing key only logs at runtime and shows the key itself.
        let (mut all, mut keys) = (String::new(), Vec::new());
        sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut all,
            &mut keys,
        );
        for k in &keys {
            assert!(super::EN.iter().any(|(e, _)| e == k), "no string for {k:?}");
        }
        for (k, _) in super::EN {
            assert!(all.contains(&format!("\"{k}\"")), "{k:?} is never shown");
        }
    }
}
