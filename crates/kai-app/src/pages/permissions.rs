//! Settings › Permissions: actual OS access and input-backend diagnostics.
//! A switch being on in macOS Settings does not guarantee the current signed
//! binary is trusted (notably after rebuilding an ad-hoc signed app).

use crate::{AgentHandle, app::KaiApp};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    feedback::InlineMessage,
    primitives::Severity,
    settings::SettingsSection,
};
use gpui::{Context, ParentElement, div};
use kai_os::permissions::Status;

pub fn refresh(app: &mut KaiApp, cx: &mut Context<KaiApp>) {
    if app.pages.permission_checking {
        return;
    }
    app.pages.permission_checking = true;
    let agent = &cx.global::<AgentHandle>().0;
    let rx = agent.permissions();
    let input = agent.refresh_input();
    cx.spawn(async move |this, cx| {
        let result = rx.await.ok();
        let _ = input.await;
        let _ = this.update(cx, |a, cx| {
            a.pages.permission_checks = result;
            a.pages.permission_checking = false;
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}

pub fn section(app: &KaiApp, cx: &Context<KaiApp>) -> SettingsSection {
    let checker = cx.entity();
    let requester = cx.entity();
    let agent = &cx.global::<AgentHandle>().0;
    let ready = agent.can_act();
    let input = if ready {
        InlineMessage::new(
            Severity::Success,
            "Mouse and keyboard backend: ready".to_string(),
        )
    } else {
        InlineMessage::new(
            Severity::Danger,
            format!(
                "Mouse and keyboard backend: {}",
                agent.input_error().unwrap_or_else(|| "initializing".into())
            ),
        )
    };
    let mut page = SettingsSection::new("Permissions")
        .description("Checks the permissions of this running app and retries its input backend.")
        .row(input)
        .row(
            Button::new(
                "refresh-access",
                if app.pages.permission_checking {
                    "Checking…"
                } else {
                    "Check permissions again"
                },
            )
            .variant(ButtonVariant::Secondary)
            .disabled(app.pages.permission_checking)
            .on_click(move |_, _, cx| checker.update(cx, refresh)),
        );
    if let Some(checks) = &app.pages.permission_checks {
        for check in checks {
            let (severity, state) = match check.status {
                Status::Granted => (Severity::Success, "Granted"),
                Status::Limited => (Severity::Warning, "Limited"),
                Status::Missing => (Severity::Danger, "Missing"),
            };
            page = page.row(InlineMessage::new(
                severity,
                format!("{}: {state}", check.name),
            ));
            if check.status != Status::Granted {
                page = page.row(div().child(check.fix.clone()));
            }
        }
    } else if !app.pages.permission_checking {
        page = page.row(InlineMessage::new(
            Severity::Warning,
            "Permission checks did not finish. Try again.".to_string(),
        ));
    }
    if cfg!(target_os = "macos") {
        page = page
            .row(Button::new("request-mac-access", "Ask macOS for access")
                .variant(ButtonVariant::Secondary)
                .on_click(move |_, _, cx| {
                    requester.update(cx, |_, cx| cx.global::<AgentHandle>().0.enable_accessibility());
                }))
            .row(div().child("If Accessibility and Screen Recording are already switched on but macOS still says Missing, quit Kai, remove its entries from those two lists using −, then add /Applications/Kai.app again using + and reopen it. Ad-hoc signed builds change identity after each rebuild."));
    }
    page
}
