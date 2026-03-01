//! Stereo VU Meter component — two thin gradient bars with ballistics and peak hold

use leptos::prelude::*;
use wasm_bindgen::{prelude::*, JsCast};

/// PPM-style decay rate: 20 dB/s (smooth analog VU feel)
const DECAY_DB_PER_SEC: f32 = 20.0;
/// Peak hold duration before decay starts (ms)
const PEAK_HOLD_MS: f64 = 1500.0;
/// Peak indicator decay rate: 20 dB/s
const PEAK_DECAY_DB_PER_SEC: f32 = 20.0;
/// Animation tick interval (~30fps)
const TICK_MS: u32 = 33;
/// Display floor — below this we show 0%
const DISPLAY_FLOOR: f32 = 0.0001;

/// Convert linear amplitude to percentage (0..100) using logarithmic scale.
/// Maps -60 dB..+6 dB → 0%..100%.
fn linear_to_pct(level: f32) -> f32 {
    if level <= DISPLAY_FLOOR {
        return 0.0;
    }
    let db = 20.0 * level.log10();
    ((db + 60.0) / 66.0 * 100.0).clamp(0.0, 100.0)
}

/// Decay factor per tick: level *= decay_factor each 33ms frame.
/// decay_factor = 10^(-DECAY_DB_PER_SEC * TICK_S / 20)
fn decay_factor() -> f32 {
    let tick_s = TICK_MS as f32 / 1000.0;
    10.0_f32.powf(-DECAY_DB_PER_SEC * tick_s / 20.0)
}

/// Peak decay factor per tick (slower than main decay).
fn peak_decay_factor() -> f32 {
    let tick_s = TICK_MS as f32 / 1000.0;
    10.0_f32.powf(-PEAK_DECAY_DB_PER_SEC * tick_s / 20.0)
}

/// Apply ballistic smoothing for one tick.
/// Returns new display level given current display and target.
fn ballistic_tick(display: f32, target: f32, decay: f32) -> f32 {
    if target > display {
        // Instant attack
        target
    } else {
        // Smooth decay
        let decayed = display * decay;
        if decayed < DISPLAY_FLOOR {
            0.0
        } else {
            decayed
        }
    }
}

/// Apply peak hold logic for one tick.
/// Returns (new_peak_level, new_hold_remaining_ms).
fn peak_hold_tick(
    peak: f32,
    hold_remaining: f64,
    target: f32,
    peak_decay: f32,
    tick_ms: f64,
) -> (f32, f64) {
    if target > peak {
        // New peak — reset hold timer
        (target, PEAK_HOLD_MS)
    } else if hold_remaining > 0.0 {
        // Holding — keep peak level, count down timer
        (peak, (hold_remaining - tick_ms).max(0.0))
    } else {
        // Hold expired — decay peak
        let decayed = peak * peak_decay;
        let new_peak = if decayed < DISPLAY_FLOOR {
            0.0
        } else {
            decayed
        };
        (new_peak, 0.0)
    }
}

/// Stereo VU meter with ballistics and peak hold
#[component]
pub fn Meter(
    /// Left channel peak level (0.0 to 1.0+)
    level_l: Signal<f32>,
    /// Right channel peak level (0.0 to 1.0+)
    level_r: Signal<f32>,
) -> impl IntoView {
    // Smoothed display values for L and R bars
    let (display_l, set_display_l) = signal(0.0_f32);
    let (display_r, set_display_r) = signal(0.0_f32);
    // Peak hold values and timers
    let (peak_l, set_peak_l) = signal(0.0_f32);
    let (peak_r, set_peak_r) = signal(0.0_f32);
    let (hold_l, set_hold_l) = signal(0.0_f64);
    let (hold_r, set_hold_r) = signal(0.0_f64);

    let decay = decay_factor();
    let p_decay = peak_decay_factor();
    let tick_ms_f64 = TICK_MS as f64;

    // 30fps animation loop using raw JS setInterval + Closure::forget().
    // gloo_timers::Interval stored in a local Rc gets dropped when the component
    // function returns, killing the timer immediately. Raw JS setInterval with
    // forget() keeps the closure alive; on_cleanup clears the interval.
    let tick_closure = Closure::wrap(Box::new(move || {
        let target_l = level_l.get_untracked();
        let target_r = level_r.get_untracked();

        // Ballistic smoothing for bar levels
        set_display_l.update(|d| *d = ballistic_tick(*d, target_l, decay));
        set_display_r.update(|d| *d = ballistic_tick(*d, target_r, decay));

        // Peak hold logic
        let cur_peak_l = peak_l.get_untracked();
        let cur_hold_l = hold_l.get_untracked();
        let (new_peak_l, new_hold_l) =
            peak_hold_tick(cur_peak_l, cur_hold_l, target_l, p_decay, tick_ms_f64);
        set_peak_l.set(new_peak_l);
        set_hold_l.set(new_hold_l);

        let cur_peak_r = peak_r.get_untracked();
        let cur_hold_r = hold_r.get_untracked();
        let (new_peak_r, new_hold_r) =
            peak_hold_tick(cur_peak_r, cur_hold_r, target_r, p_decay, tick_ms_f64);
        set_peak_r.set(new_peak_r);
        set_hold_r.set(new_hold_r);
    }) as Box<dyn FnMut()>);

    let interval_id = web_sys::window()
        .unwrap()
        .set_interval_with_callback_and_timeout_and_arguments_0(
            tick_closure.as_ref().unchecked_ref(),
            TICK_MS as i32,
        )
        .unwrap();
    // Leaks ~300 bytes of closure state; interval is cleared by on_cleanup below
    tick_closure.forget();

    on_cleanup(move || {
        if let Some(w) = web_sys::window() {
            w.clear_interval_with_handle(interval_id);
        }
    });

    // Derived percentage signals for rendering
    let bar_l_pct = move || linear_to_pct(display_l.get());
    let bar_r_pct = move || linear_to_pct(display_r.get());
    let peak_l_pct = move || linear_to_pct(peak_l.get());
    let peak_r_pct = move || linear_to_pct(peak_r.get());

    view! {
        <div class="meter-stereo">
            <div class="meter-bar">
                <div
                    class="meter-fill"
                    style=move || format!("width:{}%", bar_l_pct())
                />
                <div
                    class="meter-peak"
                    style=move || {
                        let pct = peak_l_pct();
                        if pct < 0.5 {
                            "display:none".to_string()
                        } else {
                            format!("left:{}%", pct)
                        }
                    }
                />
            </div>
            <div class="meter-bar">
                <div
                    class="meter-fill"
                    style=move || format!("width:{}%", bar_r_pct())
                />
                <div
                    class="meter-peak"
                    style=move || {
                        let pct = peak_r_pct();
                        if pct < 0.5 {
                            "display:none".to_string()
                        } else {
                            format!("left:{}%", pct)
                        }
                    }
                />
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instant_attack() {
        // New peak should jump immediately to target
        let result = ballistic_tick(0.0, 0.8, decay_factor());
        assert!(
            (result - 0.8).abs() < 0.001,
            "Attack should be instant: got {}",
            result
        );
    }

    #[test]
    fn test_smooth_decay() {
        // When target drops, display should decay gradually, not jump
        let display = 0.8;
        let target = 0.0;
        let result = ballistic_tick(display, target, decay_factor());
        assert!(
            result < display,
            "Display should decrease: {} should be < {}",
            result,
            display
        );
        assert!(
            result > 0.0,
            "Display should not drop to 0 in one tick: got {}",
            result
        );
        // Verify it's close to display * decay_factor
        let expected = display * decay_factor();
        assert!(
            (result - expected).abs() < 0.001,
            "Expected ~{}, got {}",
            expected,
            result
        );
    }

    #[test]
    fn test_peak_hold() {
        // Initial peak capture
        let (peak, hold) = peak_hold_tick(0.0, 0.0, 0.5, peak_decay_factor(), TICK_MS as f64);
        assert!((peak - 0.5).abs() < 0.001, "Peak should capture 0.5");
        assert!(
            (hold - PEAK_HOLD_MS).abs() < 0.1,
            "Hold timer should reset to {}",
            PEAK_HOLD_MS
        );

        // During hold period — peak should stay, timer counts down
        let (peak2, hold2) = peak_hold_tick(
            0.5,
            PEAK_HOLD_MS - 100.0,
            0.0,
            peak_decay_factor(),
            TICK_MS as f64,
        );
        assert!(
            (peak2 - 0.5).abs() < 0.001,
            "Peak should hold during hold period"
        );
        assert!(hold2 < PEAK_HOLD_MS - 100.0, "Timer should count down");

        // After hold expires — peak should decay
        let (peak3, _) = peak_hold_tick(0.5, 0.0, 0.0, peak_decay_factor(), TICK_MS as f64);
        assert!(peak3 < 0.5, "Peak should decay after hold expires");
        assert!(peak3 > 0.0, "Peak should not drop to 0 in one tick");
    }

    #[test]
    fn test_silence_floor() {
        // Very small values should display as 0%
        assert_eq!(linear_to_pct(0.0), 0.0);
        assert_eq!(linear_to_pct(0.00001), 0.0);
        assert_eq!(linear_to_pct(DISPLAY_FLOOR), 0.0);
    }

    #[test]
    fn test_full_scale() {
        // 1.0 linear (0 dB) should be near 90.9% (60/66 * 100)
        let pct = linear_to_pct(1.0);
        assert!(
            (pct - 90.9).abs() < 1.0,
            "1.0 linear (0 dB) should be ~90.9%, got {}",
            pct
        );
    }

    #[test]
    fn test_clipping_caps_at_100() {
        // Values above +6 dB should cap at 100%
        let pct = linear_to_pct(2.0); // +6 dB
        assert_eq!(pct, 100.0, "+6 dB should be 100%");
    }

    #[test]
    fn test_decay_converges_to_zero() {
        // After many ticks of silence, display should reach 0
        let mut display = 1.0;
        let decay = decay_factor();
        for _ in 0..200 {
            display = ballistic_tick(display, 0.0, decay);
        }
        assert_eq!(display, 0.0, "Should converge to 0 after ~6 seconds");
    }
}
