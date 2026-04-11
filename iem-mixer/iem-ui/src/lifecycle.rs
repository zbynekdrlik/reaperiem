//! Lifecycle helpers: panic hook, WebSocket watchdog staleness check, reconnect backoff.
//!
//! Pure helpers live at module root; side-effecting install functions (panic hook)
//! are in sub-sections. Unit tests cover the pure helpers only — DOM mutation
//! paths rely on simplicity and end-to-end verification.

/// Return the reconnect delay (in ms) for the given attempt number.
///
/// Schedule: 2s, 4s, 8s, 15s, then 30s forever. Matches the design in
/// docs/superpowers/specs/2026-04-11-pwa-self-healing-design.md.
pub fn backoff_delay_ms(attempt: u32) -> u32 {
    match attempt {
        0 => 2_000,
        1 => 4_000,
        2 => 8_000,
        3 => 15_000,
        _ => 30_000,
    }
}

/// Return true if the time since `last_frame_ms` exceeds `threshold_ms`.
///
/// `now_ms` and `last_frame_ms` are millisecond timestamps (e.g. from
/// `js_sys::Date::now()`). Used by the WS watchdog to detect zombie sockets.
pub fn is_stale(last_frame_ms: f64, now_ms: f64, threshold_ms: f64) -> bool {
    now_ms - last_frame_ms > threshold_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_delay_ms_follows_schedule() {
        assert_eq!(backoff_delay_ms(0), 2_000);
        assert_eq!(backoff_delay_ms(1), 4_000);
        assert_eq!(backoff_delay_ms(2), 8_000);
        assert_eq!(backoff_delay_ms(3), 15_000);
        assert_eq!(backoff_delay_ms(4), 30_000);
        assert_eq!(backoff_delay_ms(5), 30_000);
        assert_eq!(backoff_delay_ms(100), 30_000);
    }

    #[test]
    fn is_stale_below_threshold_returns_false() {
        // 29_999 ms since last frame, threshold 30_000 ms → NOT stale
        assert!(!is_stale(100.0, 30_099.0, 30_000.0));
    }

    #[test]
    fn is_stale_above_threshold_returns_true() {
        // 30_001 ms since last frame, threshold 30_000 ms → stale
        assert!(is_stale(0.0, 30_001.0, 30_000.0));
    }
}
