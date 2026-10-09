//! Kai's own files, served before Ely's through one asset source.

use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub const HAND: &str = "kai/icons/kai-hand.svg";

pub struct KaiAssets;

impl AssetSource for KaiAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            HAND => Some(Cow::Borrowed(include_bytes!("../../../assets/icons/kai-hand.svg"))),
            _ => None,
        })
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(if HAND.starts_with(path) { vec![HAND.into()] } else { Vec::new() })
    }
}
