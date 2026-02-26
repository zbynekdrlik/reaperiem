//! Pan knob component with touch-safe activation delay
//!
//! Implements the same 300ms touch-and-hold activation pattern as the Fader
//! to prevent accidental pan changes when scrolling. Includes double-tap
//! to center for mobile (since dblclick doesn't fire on touch devices).

use leptos::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, TouchEvent};

/// Activation delay in milliseconds (matches fader)
const ACTIVATION_DELAY_MS: u32 = 300;

/// Maximum time between taps for double-tap detection (ms)
const DOUBLE_TAP_MS: f64 = 300.0;

/// Maximum distance between taps for double-tap detection (px)
const DOUBLE_TAP_DISTANCE_PX: f64 = 30.0;

/// Horizontal pan slider component with touch-safe activation
///
/// Touch behavior:
/// - Touch and release before 300ms: NO change (user was scrolling)
/// - Touch and hold 300ms+: Pan activates with haptic feedback
/// - Double-tap: Centers pan (0.5)
/// - Mouse/desktop: Immediate response (no delay needed)
/// - Desktop double-click: Centers pan (0.5)
#[component]
pub fn PanKnob(
    /// Current pan value (0.0 = left, 0.5 = center, 1.0 = right) - reactive signal
    value: Signal<f32>,
    /// Called when pan changes
    on_change: Callback<f32>,
) -> impl IntoView {
    let (local_value, set_local_value) = signal(value.get_untracked());
    let (is_activated, set_is_activated) = signal(false);
    let (is_pending, set_is_pending) = signal(false);
    let (is_touch_interaction, set_is_touch_interaction) = signal(false);
    let (saved_value, set_saved_value) = signal(0.0f32);

    // Store the timeout handle so we can cancel it
    let timeout_handle: Rc<RefCell<Option<gloo_timers::callback::Timeout>>> =
        Rc::new(RefCell::new(None));

    // Double-tap detection state
    let last_tap_time: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));
    let last_tap_x: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));

    // Update local value when signal changes (but only if not actively touching)
    Effect::new(move |_| {
        if !is_activated.get() && !is_pending.get() && !is_touch_interaction.get() {
            set_local_value.set(value.get());
        }
    });

    let on_change_touch = on_change;
    let on_change_input = on_change;

    // Touch start: check for double-tap, then begin activation countdown
    let timeout_handle_start = timeout_handle.clone();
    let last_tap_time_start = last_tap_time.clone();
    let last_tap_x_start = last_tap_x.clone();
    let handle_touchstart = move |ev: TouchEvent| {
        let now = js_sys::Date::now();

        if let Some(touch) = ev.touches().get(0) {
            let x = touch.client_x() as f64;

            // Check for double-tap: two taps within time and distance thresholds
            let prev_time = *last_tap_time_start.borrow();
            let prev_x = *last_tap_x_start.borrow();
            let dt = now - prev_time;
            let dx = (x - prev_x).abs();

            if dt < DOUBLE_TAP_MS && dx < DOUBLE_TAP_DISTANCE_PX && prev_time > 0.0 {
                // Double-tap detected → center pan
                ev.prevent_default();
                set_local_value.set(0.5);
                on_change_touch.run(0.5);
                // Reset tap tracking
                *last_tap_time_start.borrow_mut() = 0.0;
                return;
            }

            // Record this tap for potential double-tap
            *last_tap_time_start.borrow_mut() = now;
            *last_tap_x_start.borrow_mut() = x;
        }

        // Mark as touch interaction and save current value
        set_is_touch_interaction.set(true);
        set_saved_value.set(local_value.get_untracked());

        set_is_pending.set(true);

        // Start activation timeout
        let timeout = gloo_timers::callback::Timeout::new(ACTIVATION_DELAY_MS, move || {
            set_is_activated.set(true);
            set_is_pending.set(false);

            // Haptic feedback on activation
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let _ = navigator.vibrate_with_duration(30);
            }
        });

        *timeout_handle_start.borrow_mut() = Some(timeout);
    };

    // Input handler: blocks value changes during touch until activated
    let handle_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input = target.dyn_into::<HtmlInputElement>().unwrap();

        // During touch interaction, block input until activation delay passes
        if is_touch_interaction.get_untracked() && !is_activated.get_untracked() {
            // Restore the saved value — pan uses 0-100 HTML range
            let restore = saved_value.get_untracked();
            input.set_value(&format!("{}", (restore * 100.0) as i32));
            set_local_value.set(restore);
            return;
        }

        let new_value: f32 = input.value().parse().unwrap_or(50.0) / 100.0;
        set_local_value.set(new_value);
        on_change_input.run(new_value);
    };

    // Touch end: reset state
    let timeout_handle_end = timeout_handle.clone();
    let handle_touchend = move |_ev: TouchEvent| {
        *timeout_handle_end.borrow_mut() = None;
        set_is_pending.set(false);
        set_is_activated.set(false);
        set_is_touch_interaction.set(false);
    };

    // Desktop double-click to center
    let handle_dblclick = move |_| {
        set_local_value.set(0.5);
        on_change.run(0.5);
    };

    view! {
        <div class="pan-container">
            <input
                type="range"
                class=move || {
                    let mut classes = vec!["pan-slider"];
                    if is_pending.get() { classes.push("activating"); }
                    if is_activated.get() { classes.push("active"); }
                    classes.join(" ")
                }
                min="0"
                max="100"
                value=move || (local_value.get() * 100.0) as i32
                on:input=handle_input
                on:touchstart=handle_touchstart
                on:touchend=handle_touchend
                on:touchcancel=move |_| {
                    *timeout_handle.borrow_mut() = None;
                    set_is_pending.set(false);
                    set_is_activated.set(false);
                    set_is_touch_interaction.set(false);
                }
                on:dblclick=handle_dblclick
                title="Pan (double-tap to center)"
            />
        </div>
    }
}
