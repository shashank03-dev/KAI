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

/// Commands that invoke an interpreter, transfer local data or access credentials
/// require review. This is a conservative safety classifier, not a shell parser:
/// ambiguous commands should be treated as high-risk, never trusted as safe.
fn shell_interpreter(word: &str) -> bool {
    let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
    matches!(
        name,
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish" | "python" | "python3"
            | "perl" | "ruby" | "node" | "osascript" | "powershell" | "pwsh"
    ) || name.starts_with("python3.")
}

/// Risk of a shell command: destructive, credential-related, remote execution
/// and uploads are high; routine read-only commands remain low.
pub fn classify_shell(cmd: &str) -> Risk {
    let c = cmd.to_ascii_lowercase();
    let words: Vec<&str> = c
        .split(|ch: char| ch.is_whitespace() || ";|&()`'\"$".contains(ch))
        .filter(|w| !w.is_empty())
        .collect();
    let deletes = words.iter().any(|w| {
        let base = w.rsplit(['/', '\\']).next().unwrap_or(w);
        DELETE_COMMANDS.contains(&base)
    });
    let inline_code = words.windows(2).any(|pair| {
        shell_interpreter(pair[0])
            && matches!(pair[1], "-c" | "-e" | "-command" | "-encodedcommand")
    });
    let pipe_to_interpreter = c.split('|').skip(1).any(|segment| {
        let parts: Vec<&str> = segment
            .split(|ch: char| ch.is_whitespace() || ";&()`'\"$".contains(ch))
            .filter(|w| !w.is_empty())
            .collect();
        parts.first().is_some_and(|first| {
            shell_interpreter(first)
                || (matches!(
                    first.rsplit(['/', '\\']).next().unwrap_or(first),
                    "env" | "sudo" | "nohup" | "timeout"
                ) && parts.iter().skip(1).take(5).any(|w| shell_interpreter(w)))
        })
    });
    // Credential files and keychain reads are high risk even without an
    // obvious network destination: their content is a secret.
    let credentials = [
        "/.ssh/", "~/.ssh", "id_rsa", "id_ed25519", ".aws/credentials",
        ".netrc", "/etc/shadow", "login.keychain", "security find-generic-password",
        "security find-internet-password", "secret-tool lookup", "pass show",
        "get-storedcredential",
    ]
    .iter()
    .any(|needle| c.replace('\\', "/").contains(needle));
    let outbound = words.iter().any(|w| {
        matches!(*w, "curl" | "wget" | "nc" | "netcat" | "scp" | "rsync")
    });
    let upload = outbound
        && (words.iter().any(|w| {
            matches!(*w, "-d" | "-t" | "--data" | "--data-binary"
                | "--upload-file" | "--post-file")
                || w.starts_with("--data=")
                || w.starts_with("--data-binary=")
                || w.starts_with("-d@")
        }) || words.iter().any(|w| matches!(*w, "nc" | "netcat" | "scp" | "rsync")));
    if deletes
        || DESTRUCTIVE_SHELL.iter().any(|p| c.contains(p))
        || inline_code
        || pipe_to_interpreter
        || c.contains("iex(")
        || words.contains(&"iex")
        || credentials
        || upload
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
    if decision.action == Action::Shell
        && classify_shell(decision.text.as_deref().unwrap_or("")) == Risk::High
        && [
            "/.ssh/", "~/.ssh", "id_rsa", "id_ed25519", ".aws/credentials",
            ".netrc", "/etc/shadow", "login.keychain", "security find-generic-password",
            "security find-internet-password", "secret-tool lookup", "pass show",
        ]
        .iter()
        .any(|p| decision.text.as_deref().unwrap_or("").to_ascii_lowercase().replace('\\', "/").contains(p))
    {
        // Reading credentials is a password operation even in Autopilot.
        return true;
    }
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
        "complete order",
        "confirm order",
        "paypal",
        "subscribe",
        "subscription",
        "donate",
        "give now",
        "tip",
        "top up",
        "recharge",
        "add payment",
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

/// A word in `hay` starts with `prefix` (suffixes allowed: "deleting"), or
/// with one of [`EFFECT_PREFIXES`] and then `prefix` ("resend").
fn starts_word(hay: &str, prefix: &str) -> bool {
    hay.match_indices(prefix).any(|(i, _)| {
        let before = &hay[..i];
        let word = before
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_alphanumeric())
            .last()
            .map_or("", |(j, _)| &before[j..]);
        word.is_empty() || EFFECT_PREFIXES.contains(&word)
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
