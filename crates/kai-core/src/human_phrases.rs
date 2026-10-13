//! The phrase tables human-check detection matches (`human.rs`): data only,
//! kept apart so the detection logic stays readable.

use crate::element::Role;

/// Phrases only a live CAPTCHA / bot challenge uses. Matched as whole words
/// in any element's name, case-insensitive.
pub(super) const CAPTCHA_PHRASES: &[&str] = &[
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
pub(super) const CAPTCHA_NAMES: &[&str] = &["captcha", "recaptcha", "hcaptcha", "turnstile"];
pub(super) const CAPTCHA_NAME_ROLES: &[Role] =
    &[Role::Checkbox, Role::TextField, Role::Button, Role::Image];
/// Text around a badge, never a challenge.
pub(super) const CAPTCHA_BADGE: &[&str] = &["protected by", "privacy", "terms"];

/// Names of a field that takes a one-time code.
pub(super) const CODE_FIELD_PHRASES: &[&str] = &[
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
pub(super) const CODE_SENT_PHRASES: &[&str] = &[
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
pub(super) const NOT_A_CODE: &[&str] = &[
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

pub(super) const DEVICE_PHRASES: &[&str] = &[
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
