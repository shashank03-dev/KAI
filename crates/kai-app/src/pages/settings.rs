//! Settings: model and server, safety, appearance, privacy, shortcuts, about.

use crate::AgentHandle;
use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, SegmentedControl},
    feedback::{ErrorView, InlineMessage, Toast},
    forms::{Choice, PasswordInput, Select, Switch, UrlInput},
    layout::ScrollArea,
    overlays::ConfirmDialog,
    primitives::{IconName, Severity},
    settings::{Appearance, KeyboardShortcutsList, SettingsLayout, SettingsSection, ThemeSelector},
    theme::{ActiveTheme, Mode, TextSize, Theme},
};
use gpui::{Context, IntoElement, ParentElement, Styled, Window, div};
use kai_core::risk::ApprovalLevel;
use kai_core::settings::{BackendChoice, MODELS, Settings};

fn row(label: &str, hint: &str, control: impl IntoElement, cx: &Context<KaiApp>) -> impl IntoElement {
    let theme = cx.theme();
    div().py_2().flex().flex_col().gap_1()
        .child(div().child(label.to_string()))
        .child(div().text_size(theme.text_size(TextSize::Sm)).text_color(theme.colors.fg_muted).child(hint.to_string()))
        .child(control)
}

fn save_keys(app: &mut KaiApp, cx: &mut Context<KaiApp>) {
    app.settings.vllm_endpoint = app.pages.endpoint.read(cx).text().trim().to_string();
    for (field, name) in [(app.pages.vllm_key.clone(), kai_os::secrets::VLLM), (app.pages.gemini_key.clone(), kai_os::secrets::GEMINI)] {
        let v = field.read(cx).text().trim().to_string();
        if !v.is_empty() {
            match kai_os::secrets::set(name, &v) {
                Ok(()) => field.update(cx, |f, cx| f.set_text("", cx)),
                Err(e) => app.toast(Toast::new("Key not saved").body(e.to_string()).severity(Severity::Danger), cx),
            }
        }
    }
    app.save_settings(cx);
}

fn test(app: &mut KaiApp, cx: &mut Context<KaiApp>) {
    save_keys(app, cx);
    app.pages.testing = true;
    app.pages.test_result = None;
    let rx = cx.global::<AgentHandle>().0.test_connection(app.settings.clone(), kai_os::secrets::keys());
    cx.spawn(async move |this, cx| {
        let r = rx.await.unwrap_or_else(|_| Err("Kai's agent stopped.".into()));
        let _ = this.update(cx, |a, cx| {
            a.pages.testing = false;
            a.pages.test_result = Some(r.map(|(d, b)| format!("{b} answered an image request in {} ms.", d.as_millis())));
            cx.notify();
        });
    })
    .detach();
}

fn model(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let (backend, fast, careful, saver, tester) = (cx.entity(), cx.entity(), cx.entity(), cx.entity(), cx.entity());
    let keys = kai_os::secrets::keys();
    let stored = |k: &Option<String>| if k.is_some() { "A key is stored in the OS keychain. Type a new one to replace it." } else { "No key stored yet." };
    let choice = match app.settings.backend {
        BackendChoice::Both => "both",
        BackendChoice::Vllm => "vllm",
        BackendChoice::Gemini => "gemini",
        BackendChoice::Mock => "mock",
    };
    let result = match &app.pages.test_result {
        Some(Ok(m)) => Some(InlineMessage::new(Severity::Success, m.clone()).into_any_element()),
        Some(Err(e)) => Some(ErrorView::new("test-err", IconName::TriangleAlert, "The test didn't pass").detail(e.clone()).into_any_element()),
        None => None,
    };
    let models = || MODELS.iter().map(|m| Choice::new(m.id, m.label));
    SettingsSection::new("Model & server")
        .description("Where Gemma 4 runs. Screenshots go only to these endpoints.")
        .row(row("Backend", "Both tries your DigitalOcean server first and falls back to Gemini for a step when it is down or slow.", SegmentedControl::new("backend", choice)
            .segment("both", "Both", None).segment("vllm", "DigitalOcean", None).segment("gemini", "Gemini", None)
            .on_change(move |v, _, cx| backend.update(cx, |a, cx| {
                a.settings.backend = match v.as_ref() { "vllm" => BackendChoice::Vllm, "gemini" => BackendChoice::Gemini, _ => BackendChoice::Both };
                a.save_settings(cx);
            })), cx))
        .row(row("DigitalOcean server", "The address deploy/digitalocean/up.sh printed, ending in /v1.", UrlInput::new(&app.pages.endpoint), cx))
        .row(row("Server API key", stored(&keys.vllm), PasswordInput::new(&app.pages.vllm_key), cx))
        .row(row("Gemini API key", stored(&keys.gemini), PasswordInput::new(&app.pages.gemini_key), cx))
        .row(row("Fast model", "Used for every step.", Select::new("fast", models()).selected(app.settings.fast_model.clone()).on_change(move |v, _, cx| fast.update(cx, |a, cx| { a.settings.fast_model = v.to_string(); a.save_settings(cx); })), cx))
        .row(row("Careful model", "Used for one step after a failed one, then Kai drops back.", Select::new("careful", models()).selected(app.settings.careful_model.clone()).on_change(move |v, _, cx| careful.update(cx, |a, cx| { a.settings.careful_model = v.to_string(); a.save_settings(cx); })), cx))
        .row(div().flex().gap_2()
            .child(Button::new("save-model", "Save").variant(ButtonVariant::Secondary).on_click(move |_, _, cx| saver.update(cx, |a, cx| save_keys(a, cx))))
            .child(Button::new("test-model", "Save and test").variant(ButtonVariant::Primary).loading(app.pages.testing).on_click(move |_, _, cx| tester.update(cx, |a, cx| test(a, cx)))))
        .row(div().children(result))
}

fn safety(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let (level, steps, team) = (cx.entity(), cx.entity(), cx.entity());
    let sel = match app.settings.approval { ApprovalLevel::Every => "every", ApprovalLevel::HighRiskOnly => "high", ApprovalLevel::Autopilot => "auto" };
    SettingsSection::new("Safety")
        .description("Payments and passwords always ask, whatever you choose here.")
        .row(row("Ask before", "Risky means sending, buying, deleting, submitting, changing settings, or typing into a password field.", SegmentedControl::new("level", sel)
            .segment("every", "Every action", None).segment("high", "Risky actions", None).segment("auto", "Autopilot", None)
            .on_change(move |v, _, cx| level.update(cx, |a, cx| {
                a.settings.approval = match v.as_ref() { "every" => ApprovalLevel::Every, "auto" => ApprovalLevel::Autopilot, _ => ApprovalLevel::HighRiskOnly };
                a.save_settings(cx);
            })), cx))
        .row(row("Limits", &format!("At most {} steps and {} minutes per task.", app.settings.max_steps, app.settings.max_minutes), SegmentedControl::new("limits", app.settings.max_steps.to_string())
            .segment("20", "20 steps", None).segment("40", "40 steps", None).segment("80", "80 steps", None)
            .on_change(move |v, _, cx| steps.update(cx, |a, cx| { a.settings.max_steps = v.parse().unwrap_or(40); a.save_settings(cx); })), cx))
        .row(row("Stop shortcut", &format!("{} from any app, or the pointer in the top-left corner.", app.settings.kill_hotkey), div(), cx))
        .row(Switch::new("team-default", app.settings.team_mode).label("Use sub-agents by default").on_change(move |v, _, cx| team.update(cx, |a, cx| { a.settings.team_mode = v; a.team = v; a.save_settings(cx); })))
}

fn appearance(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let me = cx.entity();
    let current = match app.settings.theme.as_str() { "dark" => Appearance::Dark, "light" => Appearance::Light, _ => Appearance::System };
    SettingsSection::new("Appearance").row(ThemeSelector::new("theme-sel", current, move |a, _, cx| {
        let mode = match a {
            Appearance::Dark => Some(Mode::Dark),
            Appearance::Light => Some(Mode::Light),
            Appearance::System => None,
        };
        if let Some(m) = mode {
            Theme::set_mode(m, cx);
        }
        me.update(cx, |app, cx| {
            app.settings.theme = match a { Appearance::Dark => "dark", Appearance::Light => "light", Appearance::System => "system" }.into();
            app.save_settings(cx);
        })
    }))
}

fn privacy(app: &KaiApp, cx: &mut Context<KaiApp>) -> SettingsSection {
    let asker = cx.entity();
    SettingsSection::new("Privacy")
        .description("Kai sends screenshots only to your model endpoint. Traces never leave this computer.")
        .row(row("Traces folder", &Settings::traces_dir().display().to_string(), div(), cx))
        .row(row("Saved runs", &format!("{} runs on disk.", app.pages.history.len()), Button::new("delete-all", "Delete all history").variant(ButtonVariant::Danger).on_click(move |_, _, cx| {
            asker.update(cx, |a, cx| { a.pages.confirm_delete = true; cx.notify(); })
        }), cx))
}

fn about(cx: &mut Context<KaiApp>) -> SettingsSection {
    SettingsSection::new("About")
        .row(row("Kai", concat!("Version ", env!("CARGO_PKG_VERSION"), " · MIT licence"), div(), cx))
        .row(row("Credits", "Gemma 4 by Google DeepMind (Gemma Terms of Use). UI by Ely GPUI Components (MIT) on GPUI. Icons by Lucide (ISC). Fonts: Inter, JetBrains Mono, IBM Plex Sans (OFL).", div(), cx))
}

pub fn render(app: &mut KaiApp, _: &mut Window, cx: &mut Context<KaiApp>) -> impl IntoElement {
    let section = app.pages.settings_section.clone();
    let page = match section.as_str() {
        "safety" => safety(app, cx).into_any_element(),
        "appearance" => appearance(app, cx).into_any_element(),
        "privacy" => privacy(app, cx).into_any_element(),
        "shortcuts" => SettingsSection::new("Shortcuts").row(KeyboardShortcutsList::new(crate::pages::shortcut_list(), &app.pages.shortcut_search)).into_any_element(),
        "about" => about(cx).into_any_element(),
        _ => model(app, cx).into_any_element(),
    };
    let picker = cx.entity();
    let confirm = app.pages.confirm_delete.then(|| {
        let (closer, doer) = (cx.entity(), cx.entity());
        ConfirmDialog::new("confirm-delete", "Delete all history?", "Every saved run and its screenshots are removed from this computer. This can't be undone.", move |_, cx| {
            closer.update(cx, |a, cx| { a.pages.confirm_delete = false; cx.notify(); })
        })
        .confirm("Delete all")
        .destructive()
        .on_confirm(move |_, cx| doer.update(cx, |a, cx| {
            let r = std::fs::remove_dir_all(Settings::traces_dir());
            a.pages.reload_history();
            a.pages.open_trace = None;
            let t = match r {
                Ok(()) => Toast::new("History deleted").severity(Severity::Success),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Toast::new("History deleted").severity(Severity::Success),
                Err(e) => Toast::new("Couldn't delete history").body(e.to_string()).severity(Severity::Danger),
            };
            a.toast(t, cx);
        }))
    });
    div().size_full()
        .child(SettingsLayout::new("settings", [
            ("model", "Model & server", IconName::Server),
            ("safety", "Safety", IconName::Shield),
            ("appearance", "Appearance", IconName::Sun),
            ("privacy", "Privacy", IconName::Lock),
            ("shortcuts", "Shortcuts", IconName::Keyboard),
            ("about", "About", IconName::Info),
        ], section, move |key, _, cx| picker.update(cx, |a, cx| { a.pages.settings_section = key.to_string(); cx.notify(); }))
            .page(ScrollArea::new("settings-page").vertical().child(page)))
        .children(confirm)
}
