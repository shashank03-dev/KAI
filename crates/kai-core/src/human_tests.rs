use super::*;
use crate::element::Rect;

fn el(mark: u32, role: Role, name: &str) -> Element {
    Element {
        mark,
        role,
        name: name.into(),
        bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
        enabled: true,
        secure: false,
        value: None,
    }
}

fn marks(els: Vec<Element>) -> Grounding {
    Grounding::Marks(els)
}

fn kind(els: Vec<Element>) -> Option<CheckKind> {
    detect(&marks(els)).map(|c| c.kind)
}

fn decision(json: &str) -> Decision {
    Decision::parse_checked(json).unwrap()
}

#[test]
fn recaptcha_checkbox_is_a_captcha() {
    let g = vec![
        el(1, Role::TextField, "Email"),
        el(2, Role::Checkbox, "I'm not a robot"),
        el(3, Role::Button, "Sign in"),
    ];
    let c = detect(&marks(g)).unwrap();
    assert_eq!(c.kind, CheckKind::Captcha);
    assert_eq!(c.evidence, "checkbox \"I'm not a robot\"");
    assert!(c.field.is_none());
    assert!(!c.accepts_code());
}

#[test]
fn curly_apostrophes_and_case_still_match() {
    assert_eq!(
        kind(vec![el(1, Role::Checkbox, "I’M NOT A ROBOT")]),
        Some(CheckKind::Captcha)
    );
    assert_eq!(
        kind(vec![el(1, Role::Checkbox, "Verify you are human")]),
        Some(CheckKind::Captcha)
    );
    assert_eq!(
        kind(vec![el(
            1,
            Role::Other,
            "Widget containing a Cloudflare security challenge"
        )]),
        Some(CheckKind::Captcha)
    );
    assert_eq!(
        kind(vec![el(1, Role::TextField, "Enter the captcha")]),
        Some(CheckKind::Captcha)
    );
}

#[test]
fn a_recaptcha_badge_on_a_normal_page_does_not_pause() {
    // Invisible reCAPTCHA: the badge frame and its footer text are on many
    // ordinary sign-in pages.
    let g = vec![
        el(1, Role::TextField, "Email"),
        el(2, Role::Other, "reCAPTCHA"),
        el(3, Role::Link, "protected by reCAPTCHA"),
        el(4, Role::Link, "Privacy - Terms"),
        el(5, Role::Button, "Next"),
    ];
    assert_eq!(kind(g), None);
}

#[test]
fn ordinary_pages_never_pause() {
    let g = vec![
        el(1, Role::Button, "Robots and humans: a history"),
        el(2, Role::Link, "Hotpot recipes"),
        el(3, Role::TextField, "Search"),
        el(4, Role::Link, "2-Step Verification"),
        el(5, Role::Link, "Two-factor authentication settings"),
        el(6, Role::Button, "Copy code"),
        el(7, Role::Other, "Source code"),
    ];
    assert_eq!(kind(g), None);
}

#[test]
fn verification_code_field_is_a_one_time_code() {
    let g = vec![
        el(1, Role::Other, "We sent a code to ••• 42"),
        el(2, Role::TextField, "Verification code"),
        el(3, Role::Button, "Verify"),
    ];
    let c = detect(&marks(g)).unwrap();
    assert_eq!(c.kind, CheckKind::OneTimeCode);
    assert_eq!(
        c.field,
        Some(CodeField {
            name: "Verification code".into(),
            role: Role::TextField
        })
    );
    assert!(c.accepts_code());
}

#[test]
fn otp_is_matched_as_a_word() {
    assert_eq!(
        kind(vec![el(1, Role::TextField, "Enter OTP")]),
        Some(CheckKind::OneTimeCode)
    );
    assert_eq!(kind(vec![el(1, Role::TextField, "Hotpot name")]), None);
}

#[test]
fn shopping_and_address_codes_are_not_one_time_codes() {
    for name in [
        "Promo code",
        "Discount code",
        "Gift card code",
        "ZIP code",
        "Postal code",
        "Coupon code",
        "Country code",
        "Referral code",
    ] {
        assert_eq!(kind(vec![el(1, Role::TextField, name)]), None, "{name}");
    }
}

#[test]
fn a_code_field_needs_to_be_an_enabled_entry() {
    let mut f = el(1, Role::TextField, "Verification code");
    f.enabled = false;
    assert_eq!(kind(vec![f]), None);
    assert_eq!(kind(vec![el(1, Role::Button, "Resend code")]), None);
}

#[test]
fn code_sent_text_offers_the_only_empty_field() {
    let g = vec![
        el(1, Role::Other, "Enter the code we sent to your phone"),
        el(2, Role::TextField, ""),
        el(3, Role::Button, "Continue"),
    ];
    let c = detect(&marks(g)).unwrap();
    assert_eq!(c.kind, CheckKind::OneTimeCode);
    assert_eq!(c.field.unwrap().role, Role::TextField);

    // Two empty fields: Kai can't know which, so the user types it.
    let g = vec![
        el(1, Role::Other, "Enter the code we sent to your phone"),
        el(2, Role::TextField, ""),
        el(3, Role::TextField, "Search"),
    ];
    let c = detect(&marks(g)).unwrap();
    assert!(c.field.is_none());
    assert!(!c.accepts_code());
}

#[test]
fn captcha_wins_over_a_code_and_a_code_over_a_device() {
    let g = vec![
        el(1, Role::Other, "Check your phone"),
        el(2, Role::TextField, "Verification code"),
        el(3, Role::Checkbox, "I'm not a robot"),
    ];
    assert_eq!(kind(g), Some(CheckKind::Captcha));
    let g = vec![
        el(1, Role::Other, "Check your phone"),
        el(2, Role::TextField, "Verification code"),
    ];
    assert_eq!(kind(g), Some(CheckKind::OneTimeCode));
    let g = vec![el(1, Role::Other, "Check your phone")];
    assert_eq!(kind(g), Some(CheckKind::DeviceApproval));
}

#[test]
fn a_grid_screen_detects_nothing() {
    let g = Grounding::Grid {
        cols: 4,
        rows: 4,
        screen: Rect::new(0.0, 0.0, 100.0, 100.0),
    };
    assert!(detect(&g).is_none());
}

#[test]
fn guard_stops_the_model_clicking_a_captcha() {
    let g = marks(vec![
        el(1, Role::Checkbox, "I'm not a robot"),
        el(2, Role::Button, "Submit"),
    ]);
    let d = decision(r#"{"thought":"tick the box","action":"click","target":1,"risk":"low"}"#);
    assert_eq!(guard(&d, &g).unwrap().kind, CheckKind::Captcha);
    let d = decision(r#"{"thought":"submit","action":"click","target":2,"risk":"high"}"#);
    assert!(guard(&d, &g).is_none());
}

#[test]
fn guard_stops_the_model_guessing_a_code() {
    let g = marks(vec![el(1, Role::TextField, "Verification code")]);
    let d = decision(
        r#"{"thought":"enter code","action":"type","target":1,"text":"123456","risk":"low"}"#,
    );
    let c = guard(&d, &g).unwrap();
    assert_eq!(c.kind, CheckKind::OneTimeCode);
    assert!(c.accepts_code());
    // Clicking into the field is fine; only typing a code is not.
    let d = decision(r#"{"thought":"focus","action":"click","target":1,"risk":"low"}"#);
    assert!(guard(&d, &g).is_none());
    // Non-target actions are never guarded.
    let d = decision(r#"{"thought":"wait","action":"wait","risk":"none"}"#);
    assert!(guard(&d, &g).is_none());
}

#[test]
fn one_time_codes_parse_strictly_and_never_print() {
    assert_eq!(OneTimeCode::parse(" 123 456 ").unwrap().expose(), "123456");
    assert_eq!(
        OneTimeCode::parse("AB12-CD34").unwrap().expose(),
        "AB12CD34"
    );
    assert!(OneTimeCode::parse("123").is_none());
    assert!(OneTimeCode::parse("1234567890123").is_none());
    assert!(OneTimeCode::parse("rm -rf /").is_none());
    assert!(OneTimeCode::parse("12;34").is_none());
    assert!(OneTimeCode::parse("").is_none());
    let c = OneTimeCode::parse("987654").unwrap();
    assert_eq!(format!("{c:?}"), REDACTED_CODE);
    assert_eq!(
        format!("{:?}", HumanReply::Code(c)),
        format!("Code({REDACTED_CODE})")
    );
}

#[test]
fn redact_blanks_a_typed_code_in_any_field() {
    let mut f = el(1, Role::TextField, "Verification code");
    f.value = Some("987 654".into());
    let mut other = el(2, Role::TextField, "Name");
    other.value = Some("Asha".into());
    let mut g = marks(vec![f, other]);
    redact(&mut g, &[OneTimeCode::parse("987654").unwrap()]);
    let text = g.as_text();
    assert!(!text.contains("987"), "{text}");
    assert!(text.contains(REDACTED_CODE));
    assert!(text.contains("Asha"));
}

#[test]
fn signatures_tell_challenges_apart() {
    let a = detect(&marks(vec![el(1, Role::Checkbox, "I'm not a robot")])).unwrap();
    let b = detect(&marks(vec![el(7, Role::Checkbox, "I'm not a robot")])).unwrap();
    let c = detect(&marks(vec![el(
        1,
        Role::Image,
        "Select all images with buses",
    )]))
    .unwrap();
    assert_eq!(
        a.signature(),
        b.signature(),
        "renumbered marks are the same check"
    );
    assert_ne!(a.signature(), c.signature());
}

#[test]
fn find_field_matches_by_name_and_role_after_renumbering() {
    let field = CodeField {
        name: "Verification code".into(),
        role: Role::TextField,
    };
    let g = marks(vec![
        el(4, Role::Button, "Verification code"),
        el(9, Role::TextField, "Verification code"),
    ]);
    assert_eq!(find_field(&g, &field), Some(9));
    assert_eq!(find_field(&marks(vec![]), &field), None);
}

#[test]
fn questions_carry_the_models_text() {
    let q = HumanCheck::question("  Which account should I use?  ");
    assert_eq!(q.kind, CheckKind::Question);
    assert_eq!(q.prompt, "Which account should I use?");
    assert!(q.accepts_answer());
    assert!(!HumanCheck::question("").prompt.is_empty());
}
