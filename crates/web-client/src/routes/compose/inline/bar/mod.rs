//! The bar's notice line.
//!
//! The row itself is the view's, and the three pieces that are not markup arrive as slots
//! from whichever side is painting.
//!
//! Scheduling is one absolute field rather than a menu of presets: "tomorrow at nine" is
//! the same answer as "2026-11-09T09:00", and a picker that starts at the current minute
//! needs no arithmetic of ours to be correct in the reader's zone. Empty means now.

/// What the green notice says after a post was queued instead of sent.
pub(super) struct Notice;

impl Notice {
    /// `2026-11-09T09:00` reads better with a space than with the `T` the input emits.
    pub(super) fn of(custom: &str) -> String {
        custom.replace('T', " ")
    }
}
