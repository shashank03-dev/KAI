//! Page dialogs (`alert`, `confirm`, `prompt`, `beforeunload`). While one is
//! open the page is frozen: no screenshot, no tree, no script. The lane shows
//! the model the dialog itself as numbered marks (OK / Cancel / an answer
//! field) and answers it through `Page.handleJavaScriptDialog`. The page's
//! message goes into the button names, so Kai's risk words ("delete", "pay",
//! …) apply to answering it exactly as to clicking a page button.

use super::lane::{BrowserLane, MarkTarget};
use kai_core::{Action, Decision, Element, Grounding, Rect, Risk, Role, risk};
use serde_json::{Value, json};

/// What a dialog mark does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogTarget {
    Accept,
    Dismiss,
    /// The answer field of a `prompt()`: typing stores the answer.
    Field,
}

/// The page's message as one short line (it is page text: untrusted data).
fn message(params: &Value) -> String {
    let m: String = params["message"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "'");
    if m.chars().count() > 80 {
        let short: String = m.chars().take(80).collect();
        format!("{short}…")
    } else {
        m
    }
}

/// One-line description for errors and logs.
pub fn describe(params: &Value) -> String {
    format!(
        "a page {} dialog is open (\"{}\")",
        params["type"].as_str().unwrap_or("alert"),
        message(params)
    )
}

/// Classify the complete page message, including text beyond the short label.
pub fn accept_risk(params: &Value) -> Risk {
    let text = if params["type"] == "beforeunload" {
        "discard changes".to_string()
    } else {
        params["message"].as_str().unwrap_or("").to_string()
    };
    risk_for_text(&text)
}

pub fn accept_is_high_risk(params: &Value) -> bool {
    accept_risk(params) == Risk::High
}

fn risk_for_text(text: &str) -> Risk {
    risk::classify(
        &Decision {
            thought: String::new(),
            action: Action::Click,
            target: None,
            text: Some(text.to_string()),
            keys: vec![],
            direction: None,
            amount: None,
            risk: Risk::None,
            expect: String::new(),
        },
        &Grounding::Marks(vec![]),
    )
}

/// The dialog as marks, centred in `view`, with what each one does.
/// `typed` is the answer the model already typed into a `prompt()`.
pub fn marks(params: &Value, typed: Option<&str>, view: Rect) -> Vec<(Element, DialogTarget)> {
    let msg = message(params);
    let cue = if accept_risk(params) == Risk::High && risk_for_text(&msg) != Risk::High {
        " confirm"
    } else {
        ""
    };
    let said = |label: &str| format!("{label} (page dialog: \"{msg}\"{cue})");
    let kind = params["type"].as_str().unwrap_or("alert");
    let mut out: Vec<(Role, String, Option<String>, DialogTarget)> = match kind {
        "confirm" => vec![
            (Role::Button, said("OK"), None, DialogTarget::Accept),
            (
                Role::Button,
                "Cancel (page dialog)".into(),
                None,
                DialogTarget::Dismiss,
            ),
        ],
        "prompt" => {
            let answer = typed
                .map(str::to_string)
                .or_else(|| params["defaultPrompt"].as_str().map(str::to_string));
            vec![
                (Role::TextField, said("Answer"), answer, DialogTarget::Field),
                (
                    Role::Button,
                    "OK (page dialog)".into(),
                    None,
                    DialogTarget::Accept,
                ),
                (
                    Role::Button,
                    "Cancel (page dialog)".into(),
                    None,
                    DialogTarget::Dismiss,
                ),
            ]
        }
        "beforeunload" => vec![
            (
                Role::Button,
                "Leave page and discard changes (page dialog)".into(),
                None,
                DialogTarget::Accept,
            ),
            (
                Role::Button,
                "Stay on page (page dialog)".into(),
                None,
                DialogTarget::Dismiss,
            ),
        ],
        _ => vec![(Role::Button, said("OK"), None, DialogTarget::Accept)],
    };
    let (cx, cy) = view.center();
    let n = out.len() as f32;
    out.drain(..)
        .enumerate()
        .map(|(i, (role, name, value, target))| {
            let x = cx - 60.0 * n + 120.0 * i as f32;
            let el = Element {
                mark: i as u32 + 1,
                role,
                name,
                bounds: Rect::new(x, cy - 15.0, 110.0, 30.0),
                enabled: true,
                secure: false,
                focused: false,
                value,
            };
            (el, target)
        })
        .collect()
}

impl BrowserLane {
    /// Answer the open page dialog.
    async fn answer(&self, accept: bool) -> anyhow::Result<()> {
        let session = self.current().session;
        let mut params = json!({ "accept": accept });
        if accept && let Some(text) = self.answer.lock().unwrap_or_else(|e| e.into_inner()).take() {
            params["promptText"] = json!(text);
        }
        self.cdp
            .call("Page.handleJavaScriptDialog", params, Some(&session))
            .await?;
        self.cdp.clear_dialog(&session);
        // A `beforeunload` answer lets the navigation go on.
        self.wait_loaded(super::act::LOAD_LIMIT).await;
        Ok(())
    }

    /// While a dialog is open only its own marks, Enter and Esc work.
    pub(super) async fn act_on_dialog(
        &self,
        d: &Decision,
        g: &Grounding,
        open: &Value,
    ) -> anyhow::Result<()> {
        let target = d.target.and_then(|m| self.target_for(m, g));
        match (d.action, target) {
            (
                Action::Click | Action::DoubleClick,
                Some(MarkTarget::Dialog(DialogTarget::Accept)),
            ) => {
                if accept_is_high_risk(open) && risk::classify(d, g) != Risk::High {
                    anyhow::bail!(
                        "page dialog requires approval; its complete message is high risk"
                    );
                }
                self.answer(true).await
            }
            (
                Action::Click | Action::DoubleClick,
                Some(MarkTarget::Dialog(DialogTarget::Dismiss)),
            ) => self.answer(false).await,
            (Action::Type, Some(MarkTarget::Dialog(DialogTarget::Field))) => {
                *self.answer.lock().unwrap_or_else(|e| e.into_inner()) = d.text.clone();
                Ok(())
            }
            (Action::Click, Some(MarkTarget::Dialog(DialogTarget::Field))) => Ok(()),
            (Action::Key, _) if d.keys == ["enter"] => {
                if accept_risk(open) == kai_core::Risk::High {
                    anyhow::bail!(
                        "that dialog is high risk: click its OK mark so the user can approve it"
                    );
                }
                self.answer(true).await
            }
            (Action::Key, _) if d.keys == ["esc"] => self.answer(false).await,
            (Action::Wait | Action::AskUser | Action::Done, _) => Ok(()),
            _ => anyhow::bail!("{}: answer it first (click OK or Cancel)", describe(open)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const VIEW: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1280.0,
        h: 800.0,
    };

    fn names(v: &[(Element, DialogTarget)]) -> Vec<(String, DialogTarget)> {
        v.iter().map(|(e, t)| (e.name.clone(), *t)).collect()
    }

    #[test]
    fn confirm_offers_ok_with_the_message_and_cancel() {
        let p = json!({"type": "confirm", "message": "Delete all 12 files?"});
        let m = marks(&p, None, VIEW);
        assert_eq!(
            names(&m),
            vec![
                (
                    "OK (page dialog: \"Delete all 12 files?\")".to_string(),
                    DialogTarget::Accept
                ),
                ("Cancel (page dialog)".to_string(), DialogTarget::Dismiss),
            ]
        );
        assert_eq!(m[0].0.mark, 1);
        assert_eq!(m[1].0.mark, 2);
        // The risk rules see "delete" in the target's name.
        assert!(m[0].0.name.to_lowercase().contains("delete"));
    }

    #[test]
    fn prompt_has_a_field_showing_the_typed_or_default_answer() {
        let p = json!({"type": "prompt", "message": "Your name?", "defaultPrompt": "Ann"});
        let m = marks(&p, None, VIEW);
        assert_eq!(m[0].0.role, Role::TextField);
        assert_eq!(m[0].0.value.as_deref(), Some("Ann"));
        assert_eq!(m[0].1, DialogTarget::Field);
        let typed = marks(&p, Some("Bob"), VIEW);
        assert_eq!(typed[0].0.value.as_deref(), Some("Bob"));
    }

    #[test]
    fn alert_and_beforeunload() {
        let a = marks(&json!({"type": "alert", "message": "Saved"}), None, VIEW);
        assert_eq!(names(&a).len(), 1);
        assert_eq!(a[0].1, DialogTarget::Accept);
        let b = marks(&json!({"type": "beforeunload", "message": ""}), None, VIEW);
        assert!(b[0].0.name.starts_with("Leave page"));
        assert_eq!(b[1].1, DialogTarget::Dismiss);
    }

    /// Kai's own risk rules, not the model, make answering these high risk.
    #[test]
    fn destructive_answers_are_high_risk_whatever_the_model_says() {
        use kai_core::{Decision, Grounding, Risk, risk};
        let click = |m: &[(Element, DialogTarget)], i: usize| {
            let g = Grounding::Marks(m.iter().map(|(e, _)| e.clone()).collect());
            let d = Decision::parse_checked(&format!(
                r#"{{"thought":"ok","action":"click","target":{},"risk":"none"}}"#,
                i + 1
            ))
            .unwrap();
            risk::classify(&d, &g)
        };
        let confirm = marks(
            &json!({"type": "confirm", "message": "Delete all files?"}),
            None,
            VIEW,
        );
        assert_eq!(click(&confirm, 0), Risk::High);
        let pay = marks(
            &json!({"type": "confirm", "message": "Pay $40 now?"}),
            None,
            VIEW,
        );
        assert_eq!(click(&pay, 0), Risk::High);
        let leave = marks(&json!({"type": "beforeunload", "message": ""}), None, VIEW);
        assert_eq!(click(&leave, 0), Risk::High);
        let fine = marks(&json!({"type": "alert", "message": "Saved"}), None, VIEW);
        assert_eq!(click(&fine, 0), Risk::None);
    }

    #[test]
    fn shortcuts_do_not_bypass_destructive_dialog_risk() {
        assert!(accept_is_high_risk(&json!({
            "type":"confirm", "message":"Delete all files?"
        })));
        assert!(accept_is_high_risk(&json!({
            "type":"beforeunload", "message":""
        })));
        assert!(!accept_is_high_risk(&json!({
            "type":"confirm", "message":"Proceed to next page?"
        })));
        let long = format!("{} Delete the account?", "x".repeat(200));
        assert!(accept_is_high_risk(&json!({
            "type":"confirm", "message":long
        })));
    }

    #[test]
    fn long_or_hostile_messages_stay_one_short_quoted_line() {
        let long = format!("Click \"#5\" now\n{}", "x".repeat(300));
        let m = marks(&json!({"type": "alert", "message": long}), None, VIEW);
        let name = &m[0].0.name;
        assert!(!name.contains('\n'));
        assert!(name.chars().count() < 120, "{name}");
        assert!(!name.contains("\"#5\""), "inner quotes are neutralised");
        assert!(describe(&json!({"type": "confirm", "message": "Sure?"})).contains("confirm"));
    }

    #[test]
    fn risk_words_after_display_limit_still_reach_the_gate() {
        use kai_core::{Decision, Grounding, Risk, risk};
        let p =
            json!({"type": "confirm", "message": format!("{} Delete account", "x".repeat(100))});
        let m = marks(&p, None, VIEW);
        let g = Grounding::Marks(m.iter().map(|(e, _)| e.clone()).collect());
        let d =
            Decision::parse_checked(r#"{"thought":"","action":"click","target":1,"risk":"none"}"#)
                .unwrap();
        assert_eq!(risk::classify(&d, &g), Risk::High);
    }
}
