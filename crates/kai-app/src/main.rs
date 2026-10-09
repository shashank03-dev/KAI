//! Kai: Gemma 4's hands on your computer. The desktop app.

mod agent;
mod app;
mod approval;
mod assets;
mod brand;
mod chrome;
mod i18n;
mod mini;
mod onboarding;
mod pages;
mod state;

use ely_gpui_component::{
    Assets,
    shell::SplashScreen,
    theme::{Mode, Theme},
};
use gpui::{
    App, AppContext, Bounds, Global, TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};
use std::sync::Arc;

/// The agent, reachable from every view.
pub struct AgentHandle(pub Arc<agent::Agent>);
impl Global for AgentHandle {}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let agent = match agent::Agent::start() {
        Ok(a) => Arc::new(a),
        Err(e) => {
            eprintln!("Kai could not start its agent runtime: {e:#}");
            std::process::exit(1);
        }
    };
    let capture_dir = std::env::var_os("KAI_CAPTURE").map(std::path::PathBuf::from);

    gpui_platform::application()
        .with_assets(Assets::before(assets::KaiAssets))
        .run(move |cx: &mut App| {
            ely_gpui_component::init(cx).expect("Ely failed to start");
            let settings = kai_core::settings::Settings::load();
            i18n::install(cx);
            brand::apply(cx);
            let mode = match settings.theme.as_str() {
                "dark" => Mode::Dark,
                "light" => Mode::Light,
                _ => system_mode(cx),
            };
            Theme::set_mode_now(mode, cx);
            cx.set_global(AgentHandle(agent.clone()));
            app::bind_keys(cx);

            let splash = SplashScreen::new(i18n::t(cx, "app.name", &[]))
                .icon(ely_gpui_component::primitives::IconName::Hand)
                .tagline(i18n::t(cx, "app.tagline", &[]))
                .status(i18n::t(cx, "splash.waking", &[]))
                .open(cx)
                .map_err(|e| log::warn!("splash: {e}"))
                .ok();

            let bounds = Bounds::centered(None, size(px(1180.0), px(780.0)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Kai".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(16.0), px(18.0))),
                }),
                window_min_size: Some(size(px(720.0), px(520.0))),
                app_id: Some("dev.kai.Kai".into()),
                ..Default::default()
            };
            let window = cx
                .open_window(options, |window, cx| {
                    cx.new(|cx| app::KaiApp::new(settings, capture_dir.clone(), window, cx))
                })
                .expect("Kai's window failed to open");
            if let Some(s) = splash {
                let _ = s.update(cx, |_, window, _| window.remove_window());
            }
            let _ = window.update(cx, |app, window, cx| app.after_open(window, cx));
            cx.activate(true);
        });
}

fn system_mode(cx: &App) -> Mode {
    match cx.window_appearance() {
        gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark => Mode::Dark,
        _ => Mode::Light,
    }
}
