//! Boundary policy shared by wheel handling and the host regression suite.
/// GPUI has applied `delta` before the custom bubble handler runs.
pub(super) fn wheel_axis(after: f32, extent: f32, delta: f32) -> (f32, bool) {
    let extent = extent.max(0.0);
    let before = (after - delta).clamp(-extent, 0.0);
    let next = after.clamp(-extent, 0.0);
    (next, next != before)
}

#[cfg(test)]
mod tests {
    use super::wheel_axis;

    #[test]
    fn wheel_at_either_boundary_bubbles_to_parent() {
        assert_eq!(wheel_axis(30.0, 100.0, 30.0), (0.0, false));
        assert_eq!(wheel_axis(-130.0, 100.0, -30.0), (-100.0, false));
        assert_eq!(wheel_axis(20.0, 0.0, 20.0), (0.0, false));
    }

    #[test]
    fn wheel_inside_or_into_range_is_consumed_and_clamped() {
        assert_eq!(wheel_axis(-30.0, 100.0, -30.0), (-30.0, true));
        assert_eq!(wheel_axis(-70.0, 100.0, 30.0), (-70.0, true));
        assert_eq!(wheel_axis(-110.0, 100.0, -30.0), (-100.0, true));
        assert_eq!(wheel_axis(10.0, 100.0, 30.0), (0.0, true));
        assert_eq!(wheel_axis(-50.0, 100.0, 0.0), (-50.0, false));
    }
}
