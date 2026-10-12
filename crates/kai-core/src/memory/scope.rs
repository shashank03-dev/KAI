//! Where a lesson applies: the apps and sites of a task, a run or the screen
//! in view, and the task's content words.

use super::Scope;
use crate::schema::Action;
use crate::trace::Trace;

/// App names recognised in task text and element lists, as single tokens
/// (`(token, canonical)`). Ambiguous words ("word", "code", "edge") are left
/// out: they would tag ordinary tasks with an app.
const KNOWN_APPS: &[(&str, &str)] = &[
    ("mousepad", "mousepad"),
    ("gedit", "gedit"),
    ("kate", "kate"),
    ("kwrite", "kwrite"),
    ("notepad", "notepad"),
    ("firefox", "firefox"),
    ("chrome", "chrome"),
    ("chromium", "chromium"),
    ("thunderbird", "thunderbird"),
    ("libreoffice", "libreoffice"),
    ("soffice", "libreoffice"),
    ("calc", "libreoffice"),
    ("pcmanfm", "pcmanfm"),
    ("nautilus", "nautilus"),
    ("thunar", "thunar"),
    ("dolphin", "dolphin"),
    ("terminal", "terminal"),
    ("xterm", "xterm"),
    ("konsole", "konsole"),
    ("blender", "blender"),
    ("gimp", "gimp"),
    ("inkscape", "inkscape"),
    ("krita", "krita"),
    ("vlc", "vlc"),
    ("evince", "evince"),
    ("okular", "okular"),
    ("vscode", "vscode"),
    ("excel", "excel"),
    ("powerpoint", "powerpoint"),
    ("outlook", "outlook"),
    ("calculator", "calculator"),
    ("slack", "slack"),
    ("spotify", "spotify"),
    ("audacity", "audacity"),
];

/// Words that say nothing about what a task is.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "then", "to", "for", "of", "in", "on", "at", "by", "with",
    "from", "into", "it", "its", "is", "be", "me", "my", "you", "your", "please", "can", "could",
    "would", "kai", "this", "that", "there", "here", "now", "some", "any", "all", "as", "so", "do",
    "use", "using", "new", "file", "app",
];

/// Stopword-filtered lowercase task tokens (≥ 3 chars or containing a
/// digit), in order, without repeats, at most 12.
pub fn task_words(task: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in task
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '.'))
        .map(|w| w.trim_matches('.'))
    {
        let keep = (w.chars().count() >= 3 || w.chars().any(|c| c.is_ascii_digit()))
            && !STOPWORDS.contains(&w);
        if keep && !out.iter().any(|o| o == w) {
            out.push(w.to_string());
        }
        if out.len() >= 12 {
            break;
        }
    }
    out
}

/// Known app names (and `extra`, e.g. apps the store already has lessons
/// for) appearing as whole words in `text`.
pub fn apps_in(text: &str, extra: &[String]) -> Vec<String> {
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).collect();
    let mut out: Vec<String> = Vec::new();
    for (tok, canon) in KNOWN_APPS {
        if tokens.contains(tok) && !out.iter().any(|o| o == canon) {
            out.push(canon.to_string());
        }
    }
    for e in extra {
        let e = e.to_lowercase();
        let hit = if e.contains(' ') {
            lower.contains(&e)
        } else {
            tokens.contains(&e.as_str())
        };
        if hit && !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

/// The canonical app name for an `open_app` text: a known app it mentions,
/// else the text itself (lowercase, at most 3 words).
pub fn app_name(open_app_text: &str) -> Option<String> {
    let known = apps_in(open_app_text, &[]);
    if let Some(a) = known.into_iter().next() {
        return Some(a);
    }
    let t = open_app_text
        .to_lowercase()
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    (!t.is_empty() && t.len() <= 40).then_some(t)
}

/// File extensions that look like domains (`notes.txt`) but are not.
const FILE_EXTS: &[&str] = &[
    "txt", "md", "pdf", "png", "jpg", "jpeg", "gif", "svg", "csv", "json", "yaml", "yml", "toml",
    "rs", "py", "js", "ts", "sh", "html", "htm", "css", "xml", "doc", "docx", "xls", "xlsx", "odt",
    "ods", "ppt", "pptx", "zip", "tar", "gz", "log", "conf", "ini", "desktop", "blend", "exe",
    "py3", "lock", "c", "h", "cpp", "go", "java", "rb", "mp3", "mp4", "wav", "webm",
];

/// The host of a URL or host-like token, lowercase and without `www.`.
pub fn domain_of(s: &str) -> Option<String> {
    let s = s.trim().to_lowercase();
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(&s);
    let host = rest
        .split(['/', '?', '#', ' ', '"', '\''])
        .next()?
        .split('@')
        .next_back()?;
    let host = host.split(':').next()?.trim_start_matches("www.");
    let labels: Vec<&str> = host.split('.').collect();
    let tld = *labels.last()?;
    let ok = labels.len() >= 2
        && labels
            .iter()
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && (2..=12).contains(&tld.len())
        && tld.chars().all(|c| c.is_ascii_alphabetic())
        && !FILE_EXTS.contains(&tld);
    // Local servers keep their address: lessons about a local test page.
    let local = host.starts_with("127.0.0.1") || host == "localhost";
    if local {
        return Some(host.to_string());
    }
    ok.then(|| host.to_string())
}

/// Sites mentioned in `text`: URLs and bare host names.
pub fn domains_in(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tok in text.split(|c: char| c.is_whitespace() || "\"'()<>,;".contains(c)) {
        let looks = tok.contains("://") || (tok.contains('.') && !tok.ends_with('.'));
        if !looks {
            continue;
        }
        if let Some(d) = domain_of(tok)
            && !out.contains(&d)
        {
            out.push(d);
        }
    }
    out
}

/// The scope of a task by its text alone.
pub fn of_task(task: &str, extra_apps: &[String]) -> Scope {
    Scope {
        apps: apps_in(task, extra_apps),
        domains: domains_in(task),
        task_words: task_words(task),
        screens: Vec::new(),
    }
}

/// Apps opened and sites visited by a run's steps, in order.
pub fn of_steps(trace: &Trace) -> (Vec<String>, Vec<String>) {
    let (mut apps, mut domains) = (Vec::<String>::new(), Vec::<String>::new());
    for s in &trace.steps {
        let text = s.decision.text.as_deref().unwrap_or("");
        match s.decision.action {
            Action::OpenApp => {
                if let Some(a) = app_name(text)
                    && !apps.contains(&a)
                {
                    apps.push(a);
                }
            }
            Action::OpenUrl => {
                if let Some(d) = domain_of(text)
                    && !domains.contains(&d)
                {
                    domains.push(d);
                }
            }
            _ => {}
        }
    }
    (apps, domains)
}

/// The scope of a whole run: the task's words, plus every app it opened or
/// mentioned and every site it visited.
pub fn of_run(trace: &Trace) -> Scope {
    let mut s = of_task(&trace.task, &[]);
    let (apps, domains) = of_steps(trace);
    s.merge(&Scope {
        apps,
        domains,
        task_words: Vec::new(),
        screens: Vec::new(),
    });
    s
}

/// A coarse signature of where the run is: the app part of the window title
/// (its last ` - ` / ` — ` / ` | ` segment) and the kinds of elements on
/// screen. A dialog opening changes it; typing into a field does not.
/// `None` without a window title.
pub fn screen_sig(title: Option<&str>, g: &crate::element::Grounding) -> Option<String> {
    let title = title?.trim();
    let cut = [" - ", " — ", " | "]
        .iter()
        .filter_map(|sep| title.rfind(sep).map(|i| i + sep.len()))
        .max()
        .unwrap_or(0);
    let app: String = title[cut..]
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if app.is_empty() {
        return None;
    }
    let mut roles: Vec<&str> = match g {
        crate::element::Grounding::Marks(els) => els.iter().map(|e| e.role.label()).collect(),
        crate::element::Grounding::Grid { .. } => vec!["grid"],
    };
    roles.sort_unstable();
    roles.dedup();
    Some(format!("{app}/{}", roles.join(",")))
}

/// The app (or site) step `i` happened in: the last app opened or site
/// visited before it, else one named in its element list, else the task's.
pub fn at_step(trace: &Trace, i: usize) -> Option<String> {
    for s in trace.steps[..i.min(trace.steps.len())].iter().rev() {
        let text = s.decision.text.as_deref().unwrap_or("");
        match s.decision.action {
            Action::OpenApp => return app_name(text),
            Action::OpenUrl => return domain_of(text),
            _ => {}
        }
    }
    if let Some(s) = trace.steps.get(i)
        && let Some(a) = apps_in(&s.elements, &[]).into_iter().next()
    {
        return Some(a);
    }
    let t = of_task(&trace.task, &[]);
    t.apps.into_iter().next().or(t.domains.into_iter().next())
}
