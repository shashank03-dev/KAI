//! The engine's questions to the user: an approval for a risky step, or
//! help at a human check. Each goes to the UI as a one-shot the user
//! answers; a closed window or dropped dialog answers Stop.

use crate::agent::{HumanPending, Pending, Ui};
use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use kai_core::engine::{ApprovalChoice, Approver};
use kai_core::human::{HumanCheck, HumanHelper, HumanReply};
use kai_core::{Decision, Grounding};

pub struct UiApprover {
    pub tx: mpsc::UnboundedSender<Ui>,
}

#[async_trait]
impl Approver for UiApprover {
    async fn approve(&self, decision: &Decision, grounding: &Grounding) -> ApprovalChoice {
        let (reply, answer) = oneshot::channel();
        let element = decision
            .target
            .and_then(|t| grounding.element(t))
            .map(|e| e.name.clone());
        if self
            .tx
            .unbounded_send(Ui::Approval(Pending {
                decision: decision.clone(),
                element,
                explain: kai_core::risk::explain(decision, grounding),
                reply,
            }))
            .is_err()
        {
            return ApprovalChoice::Stop;
        }
        // A closed window or dropped dialog counts as Stop.
        answer.await.unwrap_or(ApprovalChoice::Stop)
    }
}

/// Asks the user through the app: a desktop notification, then the mini
/// window and the main window's dialog (see `human.rs`).
pub struct UiHuman {
    pub tx: mpsc::UnboundedSender<Ui>,
}

#[async_trait]
impl HumanHelper for UiHuman {
    async fn ask(&self, check: &HumanCheck) -> HumanReply {
        let (reply, answer) = oneshot::channel();
        if self
            .tx
            .unbounded_send(Ui::Human(HumanPending {
                check: check.clone(),
                reply,
            }))
            .is_err()
        {
            return HumanReply::Stop;
        }
        // The dialog is up; now the notification (bounded, best effort).
        kai_os::notify::desktop(check.kind.title(), &check.prompt).await;
        // A closed window or dropped dialog counts as Stop.
        answer.await.unwrap_or(HumanReply::Stop)
    }
}
