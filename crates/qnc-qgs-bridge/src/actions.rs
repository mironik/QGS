//! Operator actions copied from `qnc-player-client`.
//!
//! `TogglePlayPause`, `Step`, and `Cue` require a confirmed carrier frame.
//! Step clamps into the half-open active range. Cue asks for the requested
//! frame and leaves an out-of-range target for the runtime to reject.
//! `present_frame` on the QNC wire command does not become visual verification.

pub(crate) fn stepped_frame(frame: u64, delta: i64, start: u64, end_exclusive: u64) -> u64 {
    let last = end_exclusive.saturating_sub(1).max(start);
    frame.saturating_add_signed(delta).clamp(start, last)
}

#[cfg(test)]
mod tests {
    use super::stepped_frame;

    #[test]
    fn step_clamps_inside_the_half_open_range() {
        assert_eq!(stepped_frame(10, 1, 10, 90), 11);
        assert_eq!(stepped_frame(10, 1_000, 10, 90), 89);
        assert_eq!(stepped_frame(10, -1_000, 10, 90), 10);
        assert_eq!(stepped_frame(12, 0, 10, 90), 12);
    }
}
