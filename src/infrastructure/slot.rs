//! Time-slot arithmetic used to align decode and transmit cycles to FT8/FT4 cadence.
//!
//! The helpers here convert wall-clock time into numeric slot indices and parity
//! values, making the loop scheduler deterministic across every radio frame.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// FT8 uses 15-second slots; FT4 uses the shorter 7.5-second cadence.
pub const FT8_SLOT: Duration = Duration::from_secs(15);
pub const FT4_SLOT: Duration = Duration::from_millis(7_500);

/// Returns the wait time until the next UTC slot boundary for a given slot duration.
pub fn time_until_next_boundary(now: SystemTime, slot: Duration) -> Duration {
    let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let slot_ns = slot.as_nanos();
    let now_ns = since_epoch.as_nanos();
    let rem = now_ns % slot_ns;
    if rem == 0 {
        Duration::ZERO
    } else {
        Duration::from_nanos((slot_ns - rem) as u64)
    }
}

/// Converts a wall-clock time into the numeric slot index for the given slot length.
pub fn slot_index_for_time(now: SystemTime, slot: Duration) -> u64 {
    let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    (since_epoch.as_nanos() / slot.as_nanos()) as u64
}

/// Returns whether a time belongs to an odd-numbered slot.
pub fn slot_parity_for_time(now: SystemTime, slot: Duration) -> bool {
    slot_index_for_time(now, slot) % 2 == 1
}

/// Returns whether a slot index is odd, matching the parity used in FT8 processing.
pub fn slot_parity_for_index(slot_index: u64) -> bool {
    slot_index % 2 == 1
}
