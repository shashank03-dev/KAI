//! Kai's UI strings in one place. Ely's own built-in messages use its `I18n`
//! in en-US.

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
    ("done.save_skill", "Save as skill"),
    ("done.again", "Run again"),
    ("done.trace", "View trace"),
];

pub fn install(cx: &mut App) {
    cx.set_global(
        ely_gpui_component::i18n::I18n::new("en-US").catalog("en-US", &[("kai.ready", "Ready")]),
    );
}

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
    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = super::EN.iter().map(|(k, _)| *k).collect();
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), n);
    }
}
