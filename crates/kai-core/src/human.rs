//! Human checks: the moments only the person at the computer can get past —
//! a CAPTCHA, a one-time code, a "approve the sign-in on your phone" screen,
//! or a question the model needs answered. Kai never tries to solve these
//! itself. The harness (not the model) spots them, pauses the run, tells the
//! user, waits for their answer and then carries on. See
//! `docs/HUMAN_CHECKS.md`.
//!
//! Everything here is OS-free and pure: detection reads the element list,
//! the answer comes back through the [`HumanHelper`] port, and a one-time
//! code lives in a [`OneTimeCode`] that never prints, serialises or reaches
//! the model.

use crate::element::{Element, Grounding, Role};
use crate::schema::{Action, Decision};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// What kind of help the run needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    /// "I'm not a robot", an image puzzle, "verify you are human".
    Captcha,
    /// A code sent by SMS, email or an authenticator app.
    OneTimeCode,
    /// Approve the sign-in on another device, or touch a security key.
    DeviceApproval,
    /// The model asked the user something (`ask_user`).
    Question,
}

impl CheckKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CheckKind::Captcha => "captcha",
            CheckKind::OneTimeCode => "one_time_code",
            CheckKind::DeviceApproval => "device_approval",
            CheckKind::Question => "question",
        }
    }

    /// A short title for a notification or dialog.
    pub fn title(self) -> &'static str {
        match self {
            CheckKind::Captcha => "Kai needs you to pass a human check",
            CheckKind::OneTimeCode => "Kai needs a verification code",
            CheckKind::DeviceApproval => "Kai needs you to approve a sign-in",
            CheckKind::Question => "Kai has a question",
        }
    }
}

/// The input a one-time code goes into, matched again by name and role
/// after the pause (marks are renumbered on every observation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeField {
    pub name: String,
    pub role: Role,
}

/// One moment that needs the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanCheck {
    pub kind: CheckKind,
    /// What to tell the user, in plain words.
    pub prompt: String,
    /// Why Kai thinks so: the element that matched (`textfield "Verification
    /// code"`), or "the model asked". Shown in the dialog and the trace.
    pub evidence: String,
    /// For a one-time code: the field Kai can type the user's code into.
    /// `None` means the user types it themselves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<CodeField>,
}

impl HumanCheck {
    /// The same challenge seen again hashes the same; a new one (another
    /// puzzle, another field) does not. Used so a check the user already
    /// handled is not raised again while it is still on screen.
    pub fn signature(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.kind.hash(&mut h);
        self.evidence.hash(&mut h);
        h.finish()
    }

    /// Whether Kai can offer "type the code for me".
    pub fn accepts_code(&self) -> bool {
        self.kind == CheckKind::OneTimeCode && self.field.is_some()
    }

    /// Whether the user can type a free-text answer.
    pub fn accepts_answer(&self) -> bool {
        self.kind == CheckKind::Question
    }

    /// A question the model asked with `ask_user`.
    pub fn question(text: &str) -> Self {
        let text = text.trim();
        Self {
            kind: CheckKind::Question,
            prompt: if text.is_empty() {
                "Kai is stuck and needs your input to continue.".into()
            } else {
                text.chars().take(MAX_PROMPT_CHARS).collect()
            },
            evidence: "the model asked".into(),
            field: None,
        }
    }
}

/// A one-time code the user gave Kai. It never prints (`Debug` shows
/// `[one-time code]`), never serialises, and is only ever handed to the
/// actuator to type into the field the user saw. Built only through
/// [`OneTimeCode::parse`], so it is always a plausible code.
#[derive(Clone, PartialEq, Eq)]
pub struct OneTimeCode(String);

impl OneTimeCode {
    /// Accept 4–12 letters or digits (spaces and dashes the user typed for
    /// readability are dropped). Anything else is refused: Kai only ever
    /// types codes, never arbitrary text the user pasted by mistake.
    pub fn parse(raw: &str) -> Option<Self> {
        let code: String = raw
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .collect();
        let ok = (MIN_CODE_LEN..=MAX_CODE_LEN).contains(&code.chars().count())
            && code.chars().all(|c| c.is_ascii_alphanumeric());
        ok.then_some(Self(code))
    }

    /// The code itself, for the actuator only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for OneTimeCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(REDACTED_CODE)
    }
}

/// What the user answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HumanReply {
    /// "I did it": the user handled the step on screen themselves.
    Resolved,
    /// The user's answer to a question.
    Answer(String),
    /// The user gave Kai the code to type into [`HumanCheck::field`].
    Code(OneTimeCode),
    /// Stop the run.
    Stop,
}

/// Whoever can reach the user: the app (dialog + notification), the
/// terminal, or nothing (headless runs pass `None` and the run ends as
/// "asked user", exactly as before this feature).
#[async_trait]
pub trait HumanHelper: Send + Sync {
    /// Tell the user and wait for their answer. The engine races this
    /// against the kill switch and a timeout, so it may simply wait.
    async fn ask(&self, check: &HumanCheck) -> HumanReply;
}

/// What the trace keeps about a pause: never the answer or the code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanRecord {
    /// The step the pause was recorded at.
    pub step: u32,
    pub kind: CheckKind,
    pub evidence: String,
    /// `resolved`, `answered`, `code_typed`, `code_field_missing`, `stopped`,
    /// `timed_out` or `no_one_to_ask`.
    pub result: String,
    /// How long Kai waited for the user (not counted against the time limit).
    pub waited_ms: u64,
}

/// Shown instead of a code anywhere text is kept.
pub const REDACTED_CODE: &str = "[one-time code]";
/// A run asks the user at most this many times; then it ends as "asked user".
pub const MAX_ASKS: u32 = 6;
const MIN_CODE_LEN: usize = 4;
const MAX_CODE_LEN: usize = 12;
const MAX_PROMPT_CHARS: usize = 300;

// ---------------------------------------------------------------- detection

/// Phrases only a live CAPTCHA / bot challenge uses. Matched as whole words
/// in any element's name, case-insensitive.
const CAPTCHA_PHRASES: &[&str] = &[
    "i'm not a robot",
    "im not a robot",
    "i am not a robot",
    "verify you are human",
    "verify you're human",
    "verify that you are human",
    "confirm you are human",
    "are you a robot",
    "human verification",
    "bot verification",
    "security challenge",
    "recaptcha challenge",
    "hcaptcha challenge",
    "captcha challenge",
    "press and hold",
    "press & hold",
    "select all images",
    "select all squares",
    "enter the characters you see",
    "type the characters you see",
    "type the text you see",
];

/// Bare product names. Many ordinary sign-in pages carry an invisible
/// "protected by reCAPTCHA" badge, so these count only on something the
/// user would have to work (a box to tick, a field to fill, a puzzle image),
/// never on a link or a badge frame.
const CAPTCHA_NAMES: &[&str] = &["captcha", "recaptcha", "hcaptcha", "turnstile"];
const CAPTCHA_NAME_ROLES: &[Role] = &[Role::Checkbox, Role::TextField, Role::Button, Role::Image];
/// Text around a badge, never a challenge.
const CAPTCHA_BADGE: &[&str] = &["protected by", "privacy", "terms"];

/// Names of a field that takes a one-time code.
const CODE_FIELD_PHRASES: &[&str] = &[
    "verification code",
    "security code",
    "one-time code",
    "one time code",
    "one-time passcode",
    "one time passcode",
    "one-time password",
    "one time password",
    "otp",
    "2fa",
    "2fa code",
    "two-factor",
    "two factor",
    "2-step",
    "2-step verification",
    "authentication code",
    "authenticator code",
    "authenticator app",
    "sms code",
    "text message code",
    "enter code",
    "enter the code",
    "6-digit code",
    "6 digit code",
    "8-digit code",
    "confirmation code",
    "login code",
    "sign-in code",
    "sign in code",
    "passcode",
    "verify code",
    "mfa",
    "mfa code",
];

/// Page text saying a code was sent, for when the field itself has no
/// useful name.
const CODE_SENT_PHRASES: &[&str] = &[
    "we sent a code",
    "we sent you a code",
    "we've sent a code",
    "we have sent a code",
    "enter the code we sent",
    "enter the code sent",
    "code sent to",
    "code was sent",
    "check your email for a code",
    "check your phone for a code",
    "enter the verification code",
    "enter the 6-digit code",
];

/// Codes that are not one-time codes.
const NOT_A_CODE: &[&str] = &[
    "promo",
    "coupon",
    "discount",
    "voucher",
    "gift",
    "referral",
    "zip",
    "postal",
    "post code",
    "postcode",
    "country code",
    "area code",
    "dial code",
    "source code",
    "code editor",
    "qr code",
    "bar code",
    "barcode",
    "invite code",
    "invitation code",
    "access code for wifi",
];

const DEVICE_PHRASES: &[&str] = &[
    "approve the sign-in request",
    "approve sign-in request",
    "approve the request on your phone",
    "check your phone",
    "tap yes on your phone",
    "approve on your device",
    "open your authenticator app",
    "use your passkey",
    "touch your security key",
    "insert your security key",
    "waiting for approval",
    "confirm it's you on your phone",
];

/// Look at what is on screen and say whether it needs the user. Pure and
/// cheap (runs on every observation). Only strong phrases match, so an
/// ordinary page never pauses a run; a CAPTCHA wins over a code, a code
/// over a device approval.
pub fn detect(g: &Grounding) -> Option<HumanCheck> {
    let Grounding::Marks(els) = g else {
        // A grid has no names to read; the model's ask_user still reaches
        // the user.
        return None;
    };
    if let Some(e) = els.iter().find(|e| is_captcha(e)) {
        return Some(HumanCheck {
            kind: CheckKind::Captcha,
            prompt: "A CAPTCHA or \"verify you are human\" check is on screen. Kai won't \
                     solve it: please complete it yourself, then press \"I did it\"."
                .into(),
            evidence: describe(e),
            field: None,
        });
    }
    if let Some(check) = code_check(els) {
        return Some(check);
    }
    if let Some(e) = els.iter().find(|e| matches_any(&e.name, DEVICE_PHRASES)) {
        return Some(HumanCheck {
            kind: CheckKind::DeviceApproval,
            prompt: "The site wants you to approve this sign-in on your phone or security \
                     key. Approve it, then press \"I did it\"."
                .into(),
            evidence: describe(e),
            field: None,
        });
    }
    None
}

/// The model wants to act on a CAPTCHA widget or type into a one-time-code
/// field. Neither is allowed (Kai never solves a CAPTCHA and never guesses
/// a code), so the step becomes a human check instead. `None` for every
/// other decision.
pub fn guard(d: &Decision, g: &Grounding) -> Option<HumanCheck> {
    if !d.action.needs_target() {
        return None;
    }
    let el = d.target.and_then(|t| g.element(t))?;
    if is_captcha(el) {
        return Some(HumanCheck {
            kind: CheckKind::Captcha,
            prompt: "Kai reached a CAPTCHA and won't touch it. Please complete it \
                     yourself, then press \"I did it\"."
                .into(),
            evidence: describe(el),
            field: None,
        });
    }
    if d.action == Action::Type && is_code_field(el) {
        return Some(code_prompt(Some(el), describe(el)));
    }
    None
}

fn code_check(els: &[Element]) -> Option<HumanCheck> {
    if let Some(f) = els.iter().find(|e| is_code_field(e)) {
        return Some(code_prompt(Some(f), describe(f)));
    }
    let sent = els
        .iter()
        .find(|e| !is_entry(e.role) && matches_any(&e.name, CODE_SENT_PHRASES))?;
    // The page says a code was sent; offer to type it only when exactly one
    // empty, enabled text field is there to take it.
    let mut empty = els.iter().filter(|e| {
        e.role == Role::TextField && e.enabled && e.value.as_deref().is_none_or(str::is_empty)
    });
    let field = match (empty.next(), empty.next()) {
        (Some(f), None) => Some(f),
        _ => None,
    };
    Some(code_prompt(field, describe(sent)))
}

fn code_prompt(field: Option<&Element>, evidence: String) -> HumanCheck {
    let prompt = if field.is_some() {
        "The site sent you a verification code. Type it here and Kai will enter it, or \
         enter it on the page yourself and press \"I did it\"."
    } else {
        "The site sent you a verification code. Please enter it on the page yourself, \
         then press \"I did it\"."
    };
    HumanCheck {
        kind: CheckKind::OneTimeCode,
        prompt: prompt.into(),
        evidence,
        field: field.map(|f| CodeField {
            name: f.name.clone(),
            role: f.role,
        }),
    }
}

fn is_captcha(e: &Element) -> bool {
    if matches_any(&e.name, CAPTCHA_PHRASES) {
        return true;
    }
    CAPTCHA_NAME_ROLES.contains(&e.role)
        && matches_any(&e.name, CAPTCHA_NAMES)
        && !contains_any(&normalise(&e.name), CAPTCHA_BADGE)
}

fn is_entry(role: Role) -> bool {
    matches!(role, Role::TextField | Role::PasswordField)
}

fn is_code_field(e: &Element) -> bool {
    is_entry(e.role)
        && e.enabled
        && matches_any(&e.name, CODE_FIELD_PHRASES)
        && !contains_any(&e.name.to_lowercase(), NOT_A_CODE)
}

/// Find the code field again after the pause, by name and role.
pub fn find_field(g: &Grounding, field: &CodeField) -> Option<u32> {
    let Grounding::Marks(els) = g else {
        return None;
    };
    els.iter()
        .find(|e| e.enabled && e.role == field.role && e.name == field.name)
        .map(|e| e.mark)
}

fn describe(e: &Element) -> String {
    let name: String = e.name.chars().take(80).collect();
    format!("{} \"{}\"", e.role.label(), name)
}

/// Whole-word (phrase) match, case-insensitive: "otp" matches "OTP code"
/// but not "hotpot".
fn matches_any(name: &str, phrases: &[&str]) -> bool {
    let hay = normalise(name);
    if hay.is_empty() {
        return false;
    }
    phrases.iter().any(|p| has_phrase(&hay, &normalise(p)))
}

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// Lowercase, curly apostrophes made straight, runs of whitespace collapsed.
fn normalise(s: &str) -> String {
    s.to_lowercase()
        .replace('’', "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_phrase(hay: &str, phrase: &str) -> bool {
    if phrase.is_empty() {
        return false;
    }
    let word = |c: char| c.is_alphanumeric();
    let mut from = 0;
    while let Some(i) = hay[from..].find(phrase) {
        let start = from + i;
        let end = start + phrase.len();
        let before = hay[..start].chars().next_back().is_none_or(|c| !word(c));
        let after = hay[end..].chars().next().is_none_or(|c| !word(c));
        if before && after {
            return true;
        }
        from = start + phrase.chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// Blank any field whose value is a code Kai typed for the user, so the code
/// never reaches the model's element list or the trace.
pub fn redact(g: &mut Grounding, codes: &[OneTimeCode]) {
    if codes.is_empty() {
        return;
    }
    if let Grounding::Marks(els) = g {
        for e in els {
            if let Some(v) = &e.value {
                let compact: String = v
                    .chars()
                    .filter(|c| !c.is_whitespace() && *c != '-')
                    .collect();
                if codes.iter().any(|c| compact.contains(c.expose())) {
                    e.value = Some(REDACTED_CODE.into());
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "human_tests.rs"]
mod tests;
