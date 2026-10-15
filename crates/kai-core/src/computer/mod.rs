//! Versioned, model-independent computer tools. Legacy `Decision` remains
//! a separate wire format; unsupported semantics must never be approximated.

mod action;
mod validate;
pub use action::*;
pub use validate::validate_batch;

use crate::Rect;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ToolError {
    #[error("invalid computer-tool arguments")]
    InvalidArguments,
    #[error("computer-tool capability is unavailable")]
    Unsupported,
    #[error("the observed target is no longer current")]
    StaleObservation,
    #[error("computer-tool permission was denied")]
    PermissionDenied,
    #[error("computer-tool execution was cancelled")]
    Cancelled,
    #[error("computer-tool execution timed out")]
    TimedOut,
    #[error("computer-tool execution failed")]
    Failed,
    #[error("computer-tool execution needs the user")]
    NeedsUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationSpace {
    pub session: u64,
    pub id: u64,
    pub image_width: u32,
    pub image_height: u32,
    /// Captured region in native input units (points, pixels or CSS pixels).
    pub region: Rect,
}

impl ObservationSpace {
    pub fn point(&self, x: f64, y: f64) -> Result<(f64, f64), ToolError> {
        let r = self.region;
        if self.image_width == 0
            || self.image_height == 0
            || ![x, y, r.x as f64, r.y as f64, r.w as f64, r.h as f64]
                .iter()
                .all(|v| v.is_finite())
            || x < 0.
            || y < 0.
            || x >= self.image_width as f64
            || y >= self.image_height as f64
            || r.w <= 0.
            || r.h <= 0.
        {
            return Err(ToolError::InvalidArguments);
        }
        Ok((
            r.x as f64 + x * r.w as f64 / self.image_width as f64,
            r.y as f64 + y * r.h as f64 / self.image_height as f64,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: u64,
    pub session: u64,
    /// None means execution completed; it does not verify the task's outcome.
    pub error: Option<ToolError>,
    pub elapsed_ms: u64,
    pub requires_observation: bool,
}
