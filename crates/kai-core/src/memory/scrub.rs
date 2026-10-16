//! Keeping secrets out of memory. A lesson is stored for months and shown to
//! the model on later runs, so: emails, key-like tokens and long digit runs
//! are replaced; anything typed into a password/secure field never becomes a
//! lesson; and a model-written lesson that talks about approvals, passwords or
//! overriding instructions is refused (lessons are hints, never rules).

use crate::risk;
use crate::schema::Action;
use crate::trace::Trace;

/// Replace emails, key-like tokens and long digit runs.
pub fn scrub(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let grouped = scrub_grouped_numbers(text);
    for word in grouped.split(' ') {
        out.push(scrub_word(word));
    }
    out.join(" ")
}

/// Hide digit runs even when a card or account number is grouped using spaces
/// or hyphens. Keep surrounding punctuation and ordinary short numbers intact.
fn scrub_grouped_numbers(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut copied) = (0, 0);
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let (mut end, mut digits, mut last_digit) = (i, 0, i);
            while end < bytes.len() {
                if bytes[end].is_ascii_digit() {
                    digits += 1;
                    end += 1;
                    last_digit = end;
                } else if matches!(bytes[end], b' ' | b'-') {
                    end += 1;
                } else {
                    break;
                }
            }
            if digits >= 6 {
                let token_start = if start > copied && bytes[start - 1] == b'+' {
                    start - 1
                } else {
                    start
                };
                out.push_str(&text[copied..token_start]);
                out.push_str("[number]");
                copied = last_digit;
                i = last_digit;
                continue;
            }
        }
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    out.push_str(&text[copied..]);
    out
}

fn scrub_word(w: &str) -> String {
    let core = w.trim_matches(|c: char| "`'\"()[]{}<>,;:.!?".contains(c));
    if core.is_empty() {
        return w.to_string();
    }
    let rep = if is_email(core) {
        Some("[email]")
    } else if is_tokenish(core) {
        Some("[secret]")
    } else if long_digits(core) {
        Some("[number]")
    } else {
        None
    };
    match rep {
        Some(r) => w.replacen(core, r, 1),
        None => w.to_string(),
    }
}

fn is_email(s: &str) -> bool {
    let Some((user, host)) = s.split_once('@') else {
        return false;
    };
    !user.is_empty() && host.contains('.') && !host.starts_with('.')
}

/// Known key prefixes, or a long run of letters and digits mixed.
fn is_tokenish(s: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "sk-",
        "sk_",
        "pk_",
        "rk_",
        "vck_",
        "ghp_",
        "gho_",
        "ghs_",
        "github_pat_",
        "xox",
        "akia",
        "aiza",
        "eyj",
        "hf_",
        "glpat-",
    ];
    let l = s.to_ascii_lowercase();
    if PREFIXES.iter().any(|p| l.starts_with(p)) && s.len() >= 12 {
        return true;
    }
    let body: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-_+/=".contains(*c))
        .collect();
    let letters = body.chars().filter(|c| c.is_ascii_alphabetic()).count();
    let digits = body.chars().filter(|c| c.is_ascii_digit()).count();
    body.len() == s.len() && s.len() >= 20 && letters >= 4 && digits >= 3
}

/// Six or more digits in a row (card, phone, account, PIN-like numbers).
fn long_digits(s: &str) -> bool {
    let mut run = 0;
    for c in s.chars() {
        if c.is_ascii_digit() {
            run += 1;
            if run >= 6 {
                return true;
            }
        } else if !" -".contains(c) {
            run = 0;
        }
    }
    false
}

/// Does `text` still look like it carries a secret after [`scrub`]?
pub fn looks_secret(text: &str) -> bool {
    scrub(text) != text
}

/// Everything a run typed into a password/secure field (and the text of any
/// shell step that looks like it carries a secret).
pub fn secure_typed(trace: &Trace) -> Vec<String> {
    trace
        .steps
        .iter()
        .filter(|s| s.decision.action == Action::Type)
        .filter(|s| {
            s.decision
                .target
                .is_some_and(|t| risk::mark_is_secure_in_text(&s.elements, t))
        })
        .filter_map(|s| s.decision.text.clone())
        .filter(|t| t.trim().chars().count() >= 3)
        .collect()
}

/// Words a model-written lesson may not use: a lesson must never read like
/// a rule about safety or like an instruction override.
const CONTROL_WORDS: &[&str] = &[
    "approval",
    "approve",
    "approving",
    "kill switch",
    "stop shortcut",
    "password",
    "passcode",
    "credential",
    "api key",
    "token",
    "ignore previous",
    "ignore the",
    "ignore all",
    "system prompt",
    "without asking",
    "don't ask",
    "do not ask",
    "never ask",
];

/// May this text be stored as a lesson? Not when it carries a secret, holds
/// anything typed into a secure field, or (for anything but the user's own
/// words) talks about approvals, credentials or instruction overrides.
pub fn allowed(text: &str, secure: &[String], from_user: bool) -> bool {
    let lower = text.to_lowercase();
    if text.trim().is_empty() || looks_secret(text) {
        return false;
    }
    if secure
        .iter()
        .any(|s| lower.contains(&s.trim().to_lowercase()))
    {
        return false;
    }
    from_user || !CONTROL_WORDS.iter().any(|w| lower.contains(w))
}
