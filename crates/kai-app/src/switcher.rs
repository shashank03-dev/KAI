//! The quick model and backend switcher, used in the status bar and the
//! composer. The model list is `kai_core::settings::MODELS`, so new entries
//! show up here by themselves.

use crate::app::KaiApp;
use crate::runs::backend_label;
use ely_gpui_component::{
    buttons::ButtonVariant,
    menus::{DropdownMenu, Menu, MenuItem},
    primitives::IconName,
};
use gpui::{Context, Entity};
use kai_core::settings::{BackendChoice, MODELS, Settings};

/// The backends a person picks from (Mock is for tests and the CLI).
pub const BACKENDS: &[BackendChoice] = &[
    BackendChoice::Both,
    BackendChoice::Vllm,
    BackendChoice::Gemini,
    BackendChoice::Gateway,
    BackendChoice::OpenAi,
];

pub fn backend_key(b: BackendChoice) -> &'static str {
    match b {
        BackendChoice::Both => "both",
        BackendChoice::Vllm => "vllm",
        BackendChoice::Gemini => "gemini",
        BackendChoice::Gateway => "gateway",
        BackendChoice::OpenAi => "openai",
        BackendChoice::Mock => "mock",
    }
}

pub fn backend_from_key(k: &str) -> BackendChoice {
    BACKENDS
        .iter()
        .copied()
        .chain([BackendChoice::Mock])
        .find(|b| backend_key(*b) == k)
        .unwrap_or_default()
}

/// The short label on the switcher button.
pub fn short_label(s: &Settings) -> String {
    let model = MODELS
        .iter()
        .find(|m| m.id == s.fast_model)
        .map(|m| m.label.split(" · ").next().unwrap_or(m.label).to_string())
        .unwrap_or_else(|| s.fast_model.clone());
    format!("{model} · {}", backend_label(s.backend))
}

pub fn set_model(app: &mut KaiApp, id: &str, cx: &mut Context<KaiApp>) {
    app.settings.fast_model = id.to_string();
    app.save_settings(cx);
}

pub fn set_backend(app: &mut KaiApp, b: BackendChoice, cx: &mut Context<KaiApp>) {
    app.settings.backend = b;
    app.save_settings(cx);
    app.conn = crate::app::Conn::Unknown;
}

fn menu(app: &KaiApp, me: Entity<KaiApp>) -> Menu {
    let models = MODELS.iter().map(|m| {
        let me = me.clone();
        MenuItem::radio(m.label, app.settings.fast_model == m.id)
            .on_click(move |_, cx| me.update(cx, |a, cx| set_model(a, m.id, cx)))
    });
    let backends = BACKENDS.iter().map(|b| {
        let (me, b) = (me.clone(), *b);
        MenuItem::radio(backend_label(b), app.settings.backend == b).on_click(move |window, cx| {
            me.update(cx, |a, cx| {
                set_backend(a, b, cx);
                a.check_connection(window, cx);
            })
        })
    });
    Menu::new()
        .group("Model for every step", models)
        .group("Backend", backends)
        .item(
            MenuItem::new("Gemma runs on all backends; GPT models need AI Gateway or OpenAI")
                .disabled(true),
        )
}

/// A compact button that opens the switcher.
pub fn switcher(id: &'static str, app: &KaiApp, cx: &Context<KaiApp>) -> DropdownMenu {
    DropdownMenu::new(id, short_label(&app.settings), menu(app, cx.entity()))
        .icon(IconName::Cpu)
        .variant(ButtonVariant::Ghost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_keys_round_trip() {
        for b in BACKENDS {
            assert_eq!(backend_from_key(backend_key(*b)), *b);
        }
        assert_eq!(backend_from_key("nonsense"), BackendChoice::Both);
    }

    #[test]
    fn short_label_names_model_and_backend() {
        let s = Settings::default();
        assert!(short_label(&s).starts_with("Gemma 4 26B"));
        assert!(short_label(&s).ends_with("DigitalOcean + Gemini"));
    }
}
