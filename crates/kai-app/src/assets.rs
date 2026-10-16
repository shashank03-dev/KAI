//! Kai's own files, served before Ely's through one asset source.

use gpui::{AssetSource, SharedString};
use std::borrow::Cow;
use std::sync::Arc;

/// The cobalt orbit ribbon (transparent PNG) drawn in the orb and splash.
pub const MARK: &str = "kai/icons/kai-mark.png";
/// The full app icon (rounded tile): the runtime window icon where the OS
/// takes one (X11 and Windows; macOS uses the bundle's Kai.icns instead).
pub const APP_ICON: &str = "kai/icons/kai-app-512.png";

pub struct KaiAssets;

impl AssetSource for KaiAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            MARK => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/kai-mark.png"
            ))),
            APP_ICON => Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/kai-app-512.png"
            ))),
            _ => None,
        })
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok([MARK, APP_ICON]
            .iter()
            .filter(|p| p.starts_with(path))
            .map(|p| (*p).into())
            .collect())
    }
}

/// The app icon as raw RGBA for `WindowOptions::icon`.
pub fn app_icon() -> Arc<image::RgbaImage> {
    Arc::new(
        image::load_from_memory(include_bytes!("../../../assets/icons/kai-app-512.png"))
            .expect("the embedded app icon decodes")
            .into_rgba8(),
    )
}
