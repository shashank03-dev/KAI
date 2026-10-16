//! Risk classification and the approval policy.
//!
//! The model proposes a risk, but Kai decides. A password/secure field is
//! **always** high risk regardless of what the model said. The user's chosen
//! approval level and the action's risk together decide whether to pause for a
//! [`ToolApprovalDialog`].

use crate::element::{Grounding, Role};
use crate::schema::{Action, Decision, Risk};
use serde::{Deserialize, Serialize};

/// How much the user wants to be asked. Chosen in onboarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalLevel {
    /// Ask before every action.
    Every,
    /// Ask only for high-risk actions. (Default.)
    #[default]
    HighRiskOnly,
    /// Run without asking — but still always ask for payments and passwords.
    Autopilot,
}

/// Keywords that mark an action as high risk by its effect: sending, buying,
/// deleting, submitting, changing system settings. Matched where a word in
/// the target element's name or the decision's text/thought starts with one,
/// so stems (`delet`, `remov`) cover every inflection.
const HIGH_RISK_WORDS: &[&str] = &[
    "send",
    "buy",
    "purchas",
    "pay",
    "payment",
    "checkout",
    "order",
    "delet",
    "discard",
    "remov",
    "eras",
    "submit",
    "confirm",
    "transfer",
    "wire",
    "subscrib",
    "unsubscrib",
    "deactivat",
    "uninstall",
    "format",
    "shutdown",
    "restart",
    "sign out",
    "log out",
    "publish",
    "post",
];

/// Shell patterns that destroy data, escalate privilege or change the system.
const DESTRUCTIVE_SHELL: &[&str] = &[
    "rm -rf",
    "rm -r ",
    "rm -fr",
    "mkfs",
    "dd if=",
    "shutdown",
    "reboot",
    "format ",
    "del /s",
    "rd /s",
    "rmdir /s",
    "remove-item",
    "chmod -r",
    "chown -r",
    "sudo ",
    "runas ",
    "> /dev/sd",
    ":(){",
    "| sh",
    "| bash",
    "|sh",
    "|bash",
    "iex ",
    "invoke-expression",
    "reg delete",
    "diskpart",
    "git push --force",
    "drop table",
];

/// Commands that delete files, matched as whole words of the command (so
/// `rm old.log` asks like `rm -r` does, and "perform" or "format" do not).
const DELETE_COMMANDS: &[&str] = &[
    "rm",
    "rmdir",
    "unlink",
    "shred",
    "del",
    "erase",
    "rd",
    "trash",
    "trash-put",
    "-delete",
];

/// Risk of a shell command: destructive or privileged is high, the rest low.
pub fn classify_shell(cmd: &str) -> Risk {
    let c = cmd.to_ascii_lowercase();
    let deletes = c
        .split(|ch: char| ch.is_whitespace() || ";|&()`'\"".contains(ch))
        .map(|w| w.rsplit(['/', '\\']).next().unwrap_or(w))
        .any(|w| DELETE_COMMANDS.contains(&w));
    let credential = [
        ".ssh/",
        ".aws/credentials",
        ".netrc",
        "/etc/shadow",
        "keychain",
        "find-generic-password",
        "find-internet-password",
        "credential manager",
    ]
    .iter()
    .any(|p| c.contains(p));
    let outbound_data = [
        "--data",
        "-d @",
        "-t ",
        "--upload-file",
        "--post-file",
        "--post-data",
    ]
    .iter()
    .any(|p| c.contains(p))
        && ["curl", "wget", "scp", "rsync", "nc ", "netcat"]
            .iter()
            .any(|p| c.contains(p));
    let interpreter = |word: &str| {
        let name = word
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '/' && ch != '-')
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("");
        matches!(
            name,
            "sh" | "bash"
                | "zsh"
                | "dash"
                | "ksh"
                | "fish"
                | "perl"
                | "ruby"
                | "node"
                | "osascript"
                | "iex"
                | "invoke-expression"
        ) || name.starts_with("python")
    };
    let piped_to_interpreter = c.split('|').skip(1).any(|segment| {
        let mut words = segment.split_whitespace();
        let first = words.next().unwrap_or("");
        interpreter(first) || (first == "env" && words.next().is_some_and(interpreter))
    });
    let nested_interpreter = c
        .split(|ch: char| ch.is_whitespace() || ";&|`'\"()".contains(ch))
        .collect::<Vec<_>>()
        .windows(2)
        .any(|w| interpreter(w[0]) && w[1] == "-c");
    let expression = c.contains("iex(") || c.contains("invoke-expression(");
    if deletes
        || credential
        || outbound_data
        || piped_to_interpreter
        || (nested_interpreter
            && ["curl", "wget", "irm ", "invoke-restmethod"]
                .iter()
                .any(|p| c.contains(p)))
        || expression
        || DESTRUCTIVE_SHELL.iter().any(|p| c.contains(p))
    {
        Risk::High
    } else {
        Risk::Low
    }
}

/// The final, trusted risk for a decision given what it points at.
pub fn classify(decision: &Decision, grounding: &Grounding) -> Risk {
    if decision.action == Action::Shell {
        let cmd = decision.text.as_deref().unwrap_or("");
        return classify_shell(cmd).max(decision.risk);
    }
    // A secure/password field is always high, whatever the model claimed.
    if let Some(target) = decision.target
        && let Some(el) = grounding.element(target)
        && (el.secure || el.role == Role::PasswordField)
    {
        return Risk::High;
    }

    // Typing into a secure field by any route is high, including an
    // untargeted `type` that lands in a focused password field.
    if types_into_secure(decision, grounding) {
        return Risk::High;
    }

    // Effect keywords escalate to high where a word starts with them
    // ("deleting", "payments"; not "display" or "information"), and only for
    // a step that does something: waiting, finishing or asking cannot send
    // or delete.
    let inert = matches!(
        decision.action,
        Action::Wait | Action::Done | Action::AskUser
    );
    let haystack = split_camel(&effect_haystack(decision, grounding)).to_ascii_lowercase();
    if !inert && HIGH_RISK_WORDS.iter().any(|w| starts_word(&haystack, w)) {
        return Risk::High.max(decision.risk);
    }

    // Otherwise trust the model's own (clamped) risk, floored at None.
    decision.risk
}

/// Whether this decision must pause for approval under the chosen level.
pub fn needs_approval(level: ApprovalLevel, risk: Risk, is_payment_or_password: bool) -> bool {
    // Payments and passwords always ask, even on autopilot.
    if is_payment_or_password {
        return true;
    }
    match level {
        ApprovalLevel::Every => true,
        ApprovalLevel::HighRiskOnly => risk == Risk::High,
        ApprovalLevel::Autopilot => false,
    }
}

/// Does the decision touch a payment flow or a password field?
pub fn is_payment_or_password(decision: &Decision, grounding: &Grounding) -> bool {
    if let Some(t) = decision.target
        && targets_secure(grounding, t)
    {
        return true;
    }
    if types_into_secure(decision, grounding) {
        return true;
    }
    // Typing sensitive data (a card number, IBAN, ID number, key) sends it:
    // always the user's call (`crate::untrusted`).
    if sensitive_typed(decision).is_some() {
        return true;
    }
    // Only what the action touches counts here: the model's own `thought`
    // ("avoid password/payment") must not turn an email field into a payment,
    // and "pay" inside "display" is not a payment either.
    let mut h = decision.text.clone().unwrap_or_default();
    if let Some(el) = decision.target.and_then(|t| grounding.element(t)) {
        h.push(' ');
        h.push_str(&el.name);
    }
    let h = h.to_ascii_lowercase();
    [
        "pay",
        "payment",
        "pay now",
        "checkout",
        "buy",
        "buy now",
        "purchase",
        "place order",
        "order now",
        "paypal",
        "subscribe",
        "donate",
        "give now",
        "tip",
        "top up",
        "recharge",
        "card number",
        "cvv",
    ]
    .iter()
    .any(|w| has_phrase(&h, w))
}

/// The kind of sensitive data a `type` step would enter, if any.
pub fn sensitive_typed(d: &Decision) -> Option<&'static str> {
    (d.action == Action::Type)
        .then(|| crate::untrusted::sensitive(d.text.as_deref().unwrap_or("")))
        .flatten()
}

/// For the approval prompt, when the step needs the user's confirmation
/// because of sensitive data or a secure field: what Kai will do, the risk,
/// and how the data is used. `None` for other steps (their own description
/// is enough).
pub fn explain(d: &Decision, g: &Grounding) -> Option<String> {
    let field = d
        .target
        .and_then(|t| g.element(t))
        .map(|e| format!("the {} \"{}\"", e.role.label(), e.name))
        .unwrap_or_else(|| "the field that has focus".into());
    let what = match sensitive_typed(d) {
        Some(kind) => format!("your {kind}"),
        None if types_into_secure(d, g) => "text into a password or secure field".into(),
        None => return None,
    };
    Some(format!(
        "Kai wants to type {what} into {field}. Risk: typing it sends it to whatever app or \
         site owns that field. It is typed there once and nowhere else; Kai's lessons, \
         procedures and traces blank it out."
    ))
}

/// Word beginnings that keep an effect: "resend", "preorder", "autopay",
/// "unsubscribe" still send, order, pay and unsubscribe.
const EFFECT_PREFIXES: &[&str] = &["re", "pre", "un", "auto"];

/// Endings a stem may take and still be the same effect ("deleting",
/// "confirmation", "submitted"). Anything else is another word: "postcode"
/// is not "post", "payable" is not "pay" (review CORE-SAFE-10).
const SUFFIXES: &[&str] = &[
    "", "s", "es", "e", "ed", "d", "ing", "er", "ers", "ion", "ions", "ation", "ations", "al",
    "ure", "ment", "ments", "ted", "ting", "red", "ring",
];

/// Effect words that are also everyday nouns: they take only verb endings,
/// and never count as a label ("Order number", "Post office").
const NOUNS: &[&str] = &["order", "post", "format"];
const NOUN_SUFFIXES: &[&str] = &["", "s", "ed", "ing", "ted", "ting"];
const LABEL_WORDS: &[&str] = &[
    "number", "no", "id", "status", "history", "date", "details", "summary", "code", "tracking",
    "total", "office", "title", "box",
];

/// A word in `hay` is `prefix` plus an allowed ending ([`SUFFIXES`]), alone
/// or after one of [`EFFECT_PREFIXES`] ("resend").
fn starts_word(hay: &str, prefix: &str) -> bool {
    let noun = NOUNS.contains(&prefix);
    hay.match_indices(prefix).any(|(i, _)| {
        let before = &hay[..i];
        let word = before
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_alphanumeric())
            .last()
            .map_or("", |(j, _)| &before[j..]);
        if !(word.is_empty() || EFFECT_PREFIXES.contains(&word)) {
            return false;
        }
        let after = &hay[i + prefix.len()..];
        let end = after
            .find(|c: char| !c.is_alphanumeric())
            .unwrap_or(after.len());
        let (rest, tail) = after.split_at(end);
        if noun {
            let next = tail
                .trim_start()
                .split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap_or("");
            NOUN_SUFFIXES.contains(&rest) && !LABEL_WORDS.contains(&next)
        } else {
            SUFFIXES.contains(&rest)
        }
    })
}

/// `PlaceOrder` → `Place Order`: element names from web pages and apps are
/// often camel case, and each part is a word.
fn split_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if c.is_uppercase() && prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()) {
            out.push(' ');
        }
        out.push(c);
        prev = Some(c);
    }
    out
}

/// `phrase` appears in `hay` on word boundaries.
pub(crate) fn has_phrase(hay: &str, phrase: &str) -> bool {
    hay.match_indices(phrase).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + phrase.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric()) && !after.is_some_and(|c| c.is_alphanumeric())
    })
}

/// The text form of [`targets_secure`]: is mark `mark` a password/secure
/// field in an element list as recorded in a trace (`#3 password "PIN"
/// (secure)`)? Used where only the recorded text is left (experience memory).
pub fn mark_is_secure_in_text(elements: &str, mark: u32) -> bool {
    let prefix = format!("#{mark} ");
    elements.lines().any(|l| {
        l.starts_with(&prefix)
            && (l[prefix.len()..].starts_with("password ") || l.trim_end().ends_with("(secure)"))
    })
}

/// Would this `type` land in a password or secure field? With a target,
/// that field decides. Without one, the text goes wherever focus is: the
/// focused element decides, and when the tree names no focused element but
/// shows a secure field, Kai cannot rule it out and treats it as secure. A
/// grid has no tree, so there is nothing to read.
fn types_into_secure(d: &Decision, g: &Grounding) -> bool {
    if d.action != Action::Type {
        return false;
    }
    if let Some(t) = d.target {
        return targets_secure(g, t);
    }
    let Grounding::Marks(els) = g else {
        return false;
    };
    let secure = |e: &&crate::element::Element| e.secure || e.role == Role::PasswordField;
    // Several elements can report focus (a window and the field in it):
    // any focused secure one counts.
    if els.iter().any(|e| e.focused) {
        els.iter().any(|e| e.focused && secure(&e))
    } else {
        els.iter().any(|e| secure(&e))
    }
}

fn targets_secure(grounding: &Grounding, mark: u32) -> bool {
    grounding
        .element(mark)
        .map(|e| e.secure || e.role == Role::PasswordField)
        .unwrap_or(false)
}

fn effect_haystack(decision: &Decision, grounding: &Grounding) -> String {
    let mut s = decision.thought.clone();
    if let Some(t) = &decision.text {
        s.push(' ');
        s.push_str(t);
    }
    if let Some(mark) = decision.target
        && let Some(el) = grounding.element(mark)
    {
        s.push(' ');
        s.push_str(&el.name);
    }
    s
}

#[cfg(test)]
#[path = "risk_tests.rs"]
mod tests;
