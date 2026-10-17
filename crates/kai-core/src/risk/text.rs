//! Text matching helpers for risk and credential checks.

/// `phrase` appears in `hay` on word boundaries.
pub(crate) fn has_phrase(hay: &str, phrase: &str) -> bool {
    hay.match_indices(phrase).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + phrase.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric()) && !after.is_some_and(|c| c.is_alphanumeric())
    })
}

/// `PlaceOrder` → `Place Order`: element names from web pages and apps are
/// often camel case, and each part is a word.
pub(super) fn split_camel(s: &str) -> String {
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

/// The text form of a secure target: is mark `mark` a password/secure field
/// in an element list as recorded in a trace (`#3 password "PIN" (secure)`)?
/// Used where only the recorded text is left (experience memory).
pub fn mark_is_secure_in_text(elements: &str, mark: u32) -> bool {
    let prefix = format!("#{mark} ");
    elements.lines().any(|l| {
        l.starts_with(&prefix)
            && (l[prefix.len()..].starts_with("password ") || l.trim_end().ends_with("(secure)"))
    })
}
