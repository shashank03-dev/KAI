//! Kai's eyes and hands on Linux and Windows: screen capture, accessibility
//! trees, Set-of-Marks drawing, mouse and keyboard, app and URL launching, the
//! terminal, and the kill switch. Implements `kai_core`'s Perceptor and
//! Actuator for the real desktop.

pub mod a11y;
pub mod desktop;
pub mod exclude;
pub mod frame;
pub mod input;
pub mod kill;
pub mod launch;
pub mod marks;
pub mod permissions;
pub mod secrets;
pub mod shell;

pub use desktop::{DesktopActuator, DesktopPerceptor};
pub use kill::KillSwitch;
