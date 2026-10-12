//! Agent Skills, to the Agent Skill Open Standard (agentskills.io).
//!
//! A skill is a directory with a `SKILL.md` (YAML frontmatter + Markdown body)
//! and optional `scripts/`, `references/`, `assets/`. Only `name` + `description`
//! go into the system prompt (progressive disclosure); the body is loaded when
//! the skill is picked.
//!
//! Kai's spec-compatible extension lives inside the standard `metadata` map, as
//! a JSON string under the key `kai`, holding deterministic `steps` Kai can run
//! without a model call. Other agents ignore it; the skill stays valid.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Frontmatter fields defined by the standard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub metadata: std::collections::BTreeMap<String, String>,
    #[serde(
        default,
        rename = "allowed-tools",
        skip_serializing_if = "Option::is_none"
    )]
    pub allowed_tools: Option<String>,
}

/// A deterministic step Kai can run with no model call (the `kai` extension).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum DeterministicStep {
    OpenUrl { url: String },
    OpenApp { app: String },
    Shortcut { keys: Vec<String> },
    Type { text: String },
}

/// A loaded skill.
#[derive(Debug, Clone)]
pub struct Skill {
    pub dir: PathBuf,
    pub front: Frontmatter,
    /// The Markdown instructions after the frontmatter.
    pub body: String,
    pub steps: Vec<DeterministicStep>,
    pub builtin: bool,
}

impl Skill {
    pub fn name(&self) -> &str {
        &self.front.name
    }

    /// The one-line entry for the system prompt (progressive disclosure).
    pub fn prompt_line(&self) -> String {
        format!("- {}: {}", self.front.name, self.front.description)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SkillError {
    #[error("no SKILL.md in {0}")]
    MissingSkillMd(String),
    #[error("SKILL.md has no `---` frontmatter block")]
    NoFrontmatter,
    #[error("frontmatter is not valid YAML: {0}")]
    BadYaml(String),
    #[error("`name` is required")]
    MissingName,
    #[error("`description` is required and must be non-empty")]
    MissingDescription,
    #[error("`name` must be 1-64 lowercase [a-z0-9-], no leading/trailing/double hyphen: {0:?}")]
    BadName(String),
    #[error("`name` ({name:?}) must match the directory name ({dir:?})")]
    NameDirMismatch { name: String, dir: String },
    #[error("`description` must be <= 1024 characters (was {0})")]
    DescriptionTooLong(usize),
    #[error("`compatibility` must be <= 500 characters (was {0})")]
    CompatibilityTooLong(usize),
    #[error("kai metadata is present but not valid JSON steps: {0}")]
    BadKaiSteps(String),
}

/// Load and validate a skill directory. This *is* the validator used by
/// `kai skill validate`.
pub fn load(dir: &Path) -> Result<Skill, SkillError> {
    let md_path = dir.join("SKILL.md");
    let text = std::fs::read_to_string(&md_path)
        .map_err(|_| SkillError::MissingSkillMd(dir.display().to_string()))?;
    load_from_str(&text, dir)
}

/// Same as [`load`] but from an in-memory string (so it's unit-testable without
/// touching the filesystem).
pub fn load_from_str(text: &str, dir: &Path) -> Result<Skill, SkillError> {
    let (yaml, body) = split_frontmatter(text).ok_or(SkillError::NoFrontmatter)?;
    let front: Frontmatter =
        serde_yaml::from_str(yaml).map_err(|e| SkillError::BadYaml(e.to_string()))?;

    validate_frontmatter(&front, dir)?;

    // Parse the optional Kai steps extension from metadata.
    let steps = match front.metadata.get("kai") {
        Some(json) => parse_steps(json)?,
        None => Vec::new(),
    };

    Ok(Skill {
        dir: dir.to_path_buf(),
        front,
        body: body.trim().to_string(),
        steps,
        builtin: false,
    })
}

fn validate_frontmatter(front: &Frontmatter, dir: &Path) -> Result<(), SkillError> {
    if front.name.is_empty() {
        return Err(SkillError::MissingName);
    }
    if front.description.trim().is_empty() {
        return Err(SkillError::MissingDescription);
    }
    if !valid_name(&front.name) {
        return Err(SkillError::BadName(front.name.clone()));
    }
    if front.description.chars().count() > 1024 {
        return Err(SkillError::DescriptionTooLong(
            front.description.chars().count(),
        ));
    }
    if let Some(c) = &front.compatibility
        && c.chars().count() > 500
    {
        return Err(SkillError::CompatibilityTooLong(c.chars().count()));
    }
    // name must match the directory name.
    if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str())
        && dir_name != front.name
    {
        return Err(SkillError::NameDirMismatch {
            name: front.name.clone(),
            dir: dir_name.to_string(),
        });
    }
    Ok(())
}

/// Name rule from the spec: 1-64 chars, `[a-z0-9-]`, no leading/trailing or
/// consecutive hyphens.
pub fn valid_name(name: &str) -> bool {
    let n = name.chars().count();
    if !(1..=64).contains(&n) {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn parse_steps(json: &str) -> Result<Vec<DeterministicStep>, SkillError> {
    serde_json::from_str::<Vec<DeterministicStep>>(json)
        .map_err(|e| SkillError::BadKaiSteps(e.to_string()))
}

/// Split `---\n<yaml>\n---\n<body>`. Returns `(yaml, body)`.
fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = t.strip_prefix("---")?;
    // Allow the first line after the opening fence to be empty.
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))?;
    let end = rest.find("\n---")?;
    let yaml = &rest[..end];
    let after = &rest[end + 4..];
    let body = after
        .strip_prefix('\n')
        .or_else(|| after.strip_prefix("\r\n"))
        .unwrap_or(after);
    Some((yaml, body))
}

/// Load every skill directory under `root`, skipping ones that fail to
/// validate (logging why). Built-ins are marked.
pub fn load_dir(root: &Path, builtin: bool) -> Vec<Skill> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match load(&path) {
            Ok(mut s) => {
                s.builtin = builtin;
                out.push(s);
            }
            Err(e) => log::error!("skipping skill {}: {e}", path.display()),
        }
    }
    out.sort_by(|a, b| a.front.name.cmp(&b.front.name));
    out
}

/// Pick the most relevant skill for a task by keyword overlap between the task
/// and each skill's name + description. Returns the best match if it clears a
/// small threshold. This is the cheap keyword path; the model can also pick.
pub fn select_skill<'a>(task: &str, skills: &'a [Skill]) -> Option<&'a Skill> {
    let task_words: Vec<String> = tokenize(task);
    if task_words.is_empty() {
        return None;
    }
    let mut best: Option<(&Skill, usize)> = None;
    for sk in skills {
        let hay = format!(
            "{} {}",
            sk.front.name.replace('-', " "),
            sk.front.description
        );
        let hay_words = tokenize(&hay);
        let score = task_words
            .iter()
            .filter(|w| w.len() > 2 && !STOPWORDS.contains(&w.as_str()) && hay_words.contains(w))
            .count();
        if score > best.map(|(_, s)| s).unwrap_or(0) {
            best = Some((sk, score));
        }
    }
    best.filter(|(_, s)| *s >= 2).map(|(sk, _)| sk)
}

/// Words too common to say which skill fits.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "use", "when", "user", "wants", "that", "this", "from", "into",
    "your", "you", "my", "can", "please", "then", "what", "which", "are", "was", "all", "any",
];

fn tokenize(s: &str) -> Vec<String> {
    s.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "---\nname: open-and-search-web\ndescription: Open a browser and search the web. Use when the user wants to look something up online.\n---\n# Open and search\n\nSteps here.\n";

    #[test]
    fn loads_a_good_skill() {
        let dir = Path::new("/tmp/open-and-search-web");
        let s = load_from_str(GOOD, dir).unwrap();
        assert_eq!(s.front.name, "open-and-search-web");
        assert!(s.body.contains("Open and search"));
        assert!(s.steps.is_empty());
    }

    #[test]
    fn name_must_match_dir() {
        let dir = Path::new("/tmp/wrong-name");
        let err = load_from_str(GOOD, dir).unwrap_err();
        assert!(matches!(err, SkillError::NameDirMismatch { .. }));
    }

    #[test]
    fn rejects_bad_names() {
        for bad in ["PDF", "-x", "x-", "a--b", "", "white space"] {
            assert!(!valid_name(bad), "should reject {bad:?}");
        }
        for ok in ["pdf", "open-and-search-web", "a1", "x"] {
            assert!(valid_name(ok), "should accept {ok:?}");
        }
    }

    #[test]
    fn missing_frontmatter_errors() {
        let err = load_from_str("just a body", Path::new("/tmp/x")).unwrap_err();
        assert_eq!(err, SkillError::NoFrontmatter);
    }

    #[test]
    fn parses_kai_steps_extension() {
        let md = "---\nname: spotify-or-youtube-play\ndescription: Play music. Use to start a song.\nmetadata:\n  kai: '[{\"do\":\"open_url\",\"url\":\"https://open.spotify.com\"},{\"do\":\"shortcut\",\"keys\":[\"ctrl\",\"l\"]}]'\n---\nbody\n";
        let s = load_from_str(md, Path::new("/tmp/spotify-or-youtube-play")).unwrap();
        assert_eq!(s.steps.len(), 2);
        assert_eq!(
            s.steps[0],
            DeterministicStep::OpenUrl {
                url: "https://open.spotify.com".into()
            }
        );
    }

    #[test]
    fn bad_kai_steps_error() {
        let md = "---\nname: x\ndescription: d\nmetadata:\n  kai: 'not json'\n---\nb\n";
        let err = load_from_str(md, Path::new("/tmp/x")).unwrap_err();
        assert!(matches!(err, SkillError::BadKaiSteps(_)));
    }

    #[test]
    fn prompt_line_is_name_and_description() {
        let s = load_from_str(GOOD, Path::new("/tmp/open-and-search-web")).unwrap();
        assert!(
            s.prompt_line()
                .starts_with("- open-and-search-web: Open a browser")
        );
    }

    #[test]
    fn description_over_1024_rejected() {
        let long = "d".repeat(1025);
        let md = format!("---\nname: x\ndescription: {long}\n---\nb\n");
        let err = load_from_str(&md, Path::new("/tmp/x")).unwrap_err();
        assert!(matches!(err, SkillError::DescriptionTooLong(1025)));
    }
}
