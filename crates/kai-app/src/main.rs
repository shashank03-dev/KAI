//! Kai: Gemma 4's hands on your computer. The desktop app.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent;
mod anim;
mod app;
mod approval;
mod asks;
mod assets;
mod brand;
mod browser;
mod chrome;
mod control;
mod errors;
mod follow;
mod human_dialog;
mod i18n;
mod keys;
mod local;
mod local_models;
mod mini;
mod nav;
mod onboarding;
mod pages;
mod runs;
mod state;
mod store;
mod switcher;
mod team;
mod ui;
mod words;

use ely_gpui_component::{
    Assets,
    shell::SplashScreen,
    theme::{Mode, Theme},
};
use gpui::{
    App, AppContext, Bounds, Global, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, point, px, size,
};
use std::sync::Arc;

/// The agent, reachable from every view.
pub struct AgentHandle(pub Arc<agent::Agent>);
impl Global for AgentHandle {}

/// Keeps the global stop shortcut registered; holds the shortcut in use
/// (the fallback when the chosen one is taken), or None if none could be.
pub struct StopHotkey(pub Option<kai_os::kill::Hotkey>);
impl Global for StopHotkey {}

impl StopHotkey {
    /// "ctrl-alt-escape" for the status bar, or None.
    pub fn label(cx: &App) -> Option<String> {
        cx.try_global::<StopHotkey>()
            .and_then(|h| h.0.as_ref())
            .map(|h| onboarding::gh_to_gpui(&h.shortcut))
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let _instance = match single_instance() {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("Kai is already running, or could not lock its settings: {e}");
            std::process::exit(1);
        }
    };
    let agent = match agent::Agent::start() {
        Ok(a) => Arc::new(a),
        Err(e) => {
            eprintln!("Kai could not start its agent runtime: {e:#}");
            std::process::exit(1);
        }
    };
    let capture_dir = std::env::var_os("KAI_CAPTURE").map(std::path::PathBuf::from);
    let app_icon = assets::app_icon();

    gpui_platform::application()
        .with_assets(Assets::before(assets::KaiAssets))
        .run(move |cx: &mut App| {
            if let Err(e) = ely_gpui_component::init(cx) {
                fail("Kai could not start its interface", e);
            }
            let settings = kai_core::settings::Settings::load();
            brand::apply(settings.glass, cx);
            let mode = match settings.theme.as_str() {
                "dark" => Mode::Dark,
                "light" => Mode::Light,
                _ => system_mode(cx),
            };
            Theme::set_mode_now(mode, cx);
            cx.set_global(AgentHandle(agent.clone()));
            // The global stop shortcut, registered on the UI thread (Windows
            // needs it on the thread that runs the event loop).
            let hotkey = kai_os::kill::install_hotkey(&agent.kill, &settings.kill_hotkey)
                .map_err(|e| log::error!("stop shortcut not registered: {e:#}"))
                .ok();
            cx.set_global(StopHotkey(hotkey));
            app::bind_keys(cx);

            let splash = SplashScreen::new(i18n::t(cx, "app.name", &[]))
                .icon_image(assets::MARK)
                .tagline(i18n::t(cx, "app.tagline", &[]))
                .status(i18n::t(cx, "splash.waking", &[]))
                .open(cx)
                .map_err(|e| log::warn!("splash: {e}"))
                .ok();

            let audit_size = capture_dir
                .as_ref()
                .and_then(|_| std::env::var("KAI_CAPTURE_SIZE").ok())
                .and_then(|s| {
                    s.split_once('x')
                        .and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)))
                })
                .filter(|(w, h)| w.is_finite() && h.is_finite() && *w >= 720. && *h >= 520.);
            let (w, h) = audit_size.unwrap_or((1180., 780.));
            let bounds = Bounds::centered(None, size(px(w), px(h)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Kai".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(16.0), px(18.0))),
                }),
                window_min_size: Some(size(px(720.0), px(520.0))),
                app_id: Some("dev.kai.Kai".into()),
                icon: Some(app_icon),
                window_background: material(settings.glass),
                ..Default::default()
            };
            let window = match cx.open_window(options, |window, cx| {
                cx.new(|cx| app::KaiApp::new(settings, capture_dir.clone(), window, cx))
            }) {
                Ok(w) => w,
                Err(e) => fail("Kai could not open its window", e),
            };
            if let Some(s) = splash {
                let _ = s.update(cx, |_, window, _| window.remove_window());
            }
            let _ = window.update(cx, |app, window, cx| app.after_open(window, cx));
            cx.activate(true);
        });
}

/// A start-up failure the user (or a script) can act on: what failed and
/// why on stderr, and exit code 1, like a failed agent start.
fn fail(what: &str, e: impl std::fmt::Display) -> ! {
    eprintln!("{what}: {e:#}");
    std::process::exit(1)
}

/// Held until the GUI exits. Keep the lock file itself in place: deleting it
/// would allow another process to lock a new inode while this one still runs.
fn single_instance() -> std::io::Result<std::fs::File> {
    lock_instance(&kai_core::settings::Settings::dir())
}

fn lock_instance(dir: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::create_dir_all(dir)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("kai-app.lock"))?;
    file.try_lock()?;
    Ok(file)
}

#[cfg(test)]
#[test]
fn a_second_app_cannot_take_the_first_apps_lock() {
    let dir = std::env::temp_dir().join(format!(
        "kai-app-lock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let first = lock_instance(&dir).unwrap();
    assert!(lock_instance(&dir).is_err());
    drop(first);
    assert!(lock_instance(&dir).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

/// The window material: Mica on Windows 11, compositor blur elsewhere, or
/// opaque when glass is off.
pub(crate) fn material(glass: bool) -> WindowBackgroundAppearance {
    if !glass {
        WindowBackgroundAppearance::Opaque
    } else if cfg!(windows) {
        WindowBackgroundAppearance::MicaBackdrop
    } else {
        WindowBackgroundAppearance::Blurred
    }
}

/// Light or dark, as the OS currently asks.
pub(crate) fn system_mode(cx: &App) -> Mode {
    match cx.window_appearance() {
        gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark => Mode::Dark,
        _ => Mode::Light,
    }
}
