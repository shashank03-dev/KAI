//! The browser lane: Kai's own Chromium, driven over the DevTools Protocol
//! instead of the desktop (`docs/BROWSER.md`). Same marks, same actions, same
//! safety rules; the page's real accessibility tree, background input, and
//! the page's real address for the outcome ledger.

mod act;
pub mod cdp;
pub mod dialog;
pub mod dropdown;
pub mod keys;
mod lane;
mod look;
pub mod snapshot;

pub use lane::{BrowserLane, LaneOptions, default_profile, find_browser};
