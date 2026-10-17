//! Task checks (`Config::task_checks`, `docs/VERIFICATION.md`): outcomes Kai
//! reads off the task's own words, with no model call, so the DONE gate
//! holds even when the model names no outcomes. Gemma's main live failure
//! was a false `done` (`DECISIONS.md`, real-world review): it said done
//! without writing the file the task asked for, or listed a folder in the
//! terminal when asked to show it in the file manager.
//!
//! Only unambiguous phrasings count; anything else adds no check, so the
//! gate never refuses a task it misread:
//! - an output file or folder: "write … into kai-bench/x.txt", "save it as
//!   report.pdf on my Desktop", "a file called notes.txt in my home folder",
//!   "into a new folder kai-bench/restored". A path relative to the home
//!   folder needs the task to say so ("in my home folder"); a bare file name
//!   needs a place right after it. The probe then wants the file to exist
//!   and not be empty.
//! - a folder shown in the file manager: "show (me) the X folder … in the
//!   file manager" wants a window titled with the folder's name.

use crate::probe::ProbeSpec;

/// Conservative goal-coverage test for autonomous completion. A single
/// task-derived file or folder check can never prove an additional request
/// joined by "and", "then", a newline, or a semicolon was completed.
pub fn single_goal(task: &str) -> bool {
    let task = task.trim().strip_suffix("\nUse visible GUI controls only. Never open a terminal or execute shell commands, scripts, or code. Ask the user if blocked.").unwrap_or(task);
    let lower = task.to_ascii_lowercase();
    !lower.contains(['\n', '\r', ';', ',', '&'])
        && !lower.contains(". ")
        && !lower.contains("! ")
        && !lower.contains("? ")
        && ![
            " and ",
            " then ",
            " also ",
            " after ",
            " before ",
            " plus ",
            " as well as ",
            " along with ",
            " while ",
            " followed by ",
        ]
        .iter()
        .any(|word| lower.contains(word))
}

/// Words right before a path that make it the task's output.
const OUTPUT_WORDS: &[&str] = &["into", "as", "to", "called", "named"];
/// File types a task asks Kai to produce. Web pages are left out: "go to
/// example.com/page.html" is a visit, not an output.
const OUTPUT_EXTENSIONS: &[&str] = &[
    "txt", "md", "csv", "tsv", "json", "log", "zip", "gz", "tgz", "7z", "tar", "pdf", "png", "jpg",
    "jpeg", "webp", "gif", "svg", "ods", "odt", "odp", "xlsx", "docx", "pptx", "eml", "ics", "vcf",
    "py", "sh", "ps1", "yaml", "yml", "toml", "xml", "ini", "conf", "bak", "mp3", "mp4", "wav",
];
/// Places a file name may be followed by, and the folder each means.
const PLACES: &[(&str, &str)] = &[
    ("in my home folder", "~/"),
    ("in my home directory", "~/"),
    ("on my desktop", "~/Desktop/"),
    ("on the desktop", "~/Desktop/"),
    ("in my downloads", "~/Downloads/"),
    ("in the downloads", "~/Downloads/"),
    ("in my documents", "~/Documents/"),
    ("in the documents", "~/Documents/"),
];
const FILE_MANAGERS: &[&str] = &["file manager", "file explorer", "files app", "explorer"];

/// The checks for `task`: (id, description, probe), at most one per path.
pub fn derive(task: &str) -> Vec<(String, String, ProbeSpec)> {
    let mut out: Vec<(String, String, ProbeSpec)> = Vec::new();
    for path in output_paths(task) {
        if out.iter().any(|(_, _, p)| p.label().contains(&path)) {
            continue;
        }
        out.push((
            format!("output{}", out.len() + 1),
            format!("{path} exists and is not empty"),
            ProbeSpec::FileNotEmpty { path },
        ));
    }
    if let Some(name) = shown_folder(task)
        && let Ok(pattern) = regex::RegexBuilder::new(&regex::escape(&name))
            .case_insensitive(true)
            .build()
    {
        out.push((
            "shown".into(),
            format!("a window titled \"{name}\" is open"),
            ProbeSpec::WindowTitle { pattern },
        ));
    }
    out
}

fn words(task: &str) -> Vec<String> {
    task.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| "\"'`()[]{}<>,;:!?".contains(c))
                .trim_end_matches('.')
                .to_string()
        })
        .collect()
}

fn has_output_extension(w: &str) -> bool {
    let name = w
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(w)
        .to_ascii_lowercase();
    name.rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && OUTPUT_EXTENSIONS.contains(&ext))
}

fn is_absolute(p: &str) -> bool {
    p.starts_with('/')
        || p.starts_with("~/")
        || p.starts_with("~\\")
        || (p.len() > 2 && p.as_bytes()[1] == b':' && matches!(p.as_bytes()[2], b'\\' | b'/'))
}

/// Output paths named in the task, resolved to `~/…` or absolute.
fn output_paths(task: &str) -> Vec<String> {
    let w = words(task);
    let lower: Vec<String> = w.iter().map(|x| x.to_lowercase()).collect();
    let says_home = task.to_lowercase().contains("home folder")
        || task.to_lowercase().contains("home directory");
    let mut out = Vec::new();
    for (i, word) in w.iter().enumerate() {
        if word.contains("://") || word.contains('@') || i == 0 {
            continue;
        }
        let prev = lower[i - 1].as_str();
        // "into a new folder kai-bench/restored"
        let new_folder = prev == "folder"
            && word.contains('/')
            && lower[..i].iter().rev().take(4).any(|x| x == "into");
        let file = OUTPUT_WORDS.contains(&prev) && has_output_extension(word);
        if !(file || new_folder) {
            continue;
        }
        let rest = lower[i + 1..].join(" ");
        let place = PLACES
            .iter()
            .find(|(p, _)| rest.starts_with(p))
            .map(|p| p.1);
        let path = if is_absolute(word) {
            word.clone()
        } else if let Some(dir) = place {
            format!("{dir}{word}")
        } else if word.contains(['/', '\\']) && says_home {
            format!("~/{word}")
        } else {
            continue;
        };
        out.push(path);
    }
    out
}

/// The folder a task asks to see in the file manager, by its last name.
fn shown_folder(task: &str) -> Option<String> {
    let lower = task.to_lowercase();
    if !FILE_MANAGERS.iter().any(|f| lower.contains(f)) {
        return None;
    }
    let w = words(task);
    let verb = w
        .iter()
        .position(|x| matches!(x.to_lowercase().as_str(), "show" | "open"))?;
    let folder = w[verb..]
        .iter()
        .position(|x| x.eq_ignore_ascii_case("folder"))?
        + verb;
    let name = w.get(folder.checked_sub(1)?)?;
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let generic = [
        "the", "a", "my", "this", "that", "new", "me", "your", "home",
    ];
    (folder > verb + 1 && !last.is_empty() && !generic.contains(&last.to_lowercase().as_str()))
        .then(|| last.to_string())
}

#[cfg(test)]
#[path = "task_checks_tests.rs"]
mod tests;
