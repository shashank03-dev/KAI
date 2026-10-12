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
pub mod local;
pub mod marks;
pub mod notify;
pub mod perceive;
pub mod permissions;
pub mod probe;
pub mod secrets;
pub mod shell;
pub mod zoom;

pub use desktop::{DesktopActuator, DesktopPerceptor};
pub use kill::KillSwitch;
pub use probe::DesktopProbe;
