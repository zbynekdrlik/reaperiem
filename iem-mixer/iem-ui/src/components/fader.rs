//! Fader component with touch-safe activation delay
//!
//! Implements a 300ms touch-and-hold activation pattern to prevent
//! accidental volume changes when scrolling. This is the SOTA pattern
//! used by professional audio apps (Soundcraft Ui, Mixing Station).

use leptos::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{HtmlInputElement, TouchEvent};

/// Activation delay in milliseconds (SOTA: 250-350ms)
const ACTIVATION_DELAY_MS: u32 = 300;

/// Horizontal fader component with touch-safe activation
///
/// Touch behavior:
/// - Touch and release before 300ms: NO change (user was scrolling)
/// - Touch and hold 300ms+: Fader activates with haptic feedback
/// - Mouse/desktop: Immediate response (no delay needed)
///
/// Note: dB value display is handled by the parent (mixer.rs) via the
/// `.db-display` element. The Fader component only renders the slider.
#[component]
pub fn Fader(
    /// Current value in dB (reactive signal)
    value: Signal<f32>,
    /// Minimum value (default -60)
    #[prop(default = -60.0)]
    min: f32,
    /// Maximum value (default 12)
    #[prop(default = 12.0)]
    max: f32,
    /// Called when value changes
    on_change: impl Fn(f32) + 'static,
) -> impl IntoView {
    let (local_value, set_local_value) = signal(value.get_untracked());
    let (is_activated, set_is_activated) = signal(false);
    let (is_pending, set_is_pending) = signal(false);

    // Store the timeout handle so we can cancel it
    let timeout_handle: Rc<RefCell<Option<gloo_timers::callback::Timeout>>> =
        Rc::new(RefCell::new(None));

    // Store initial touch position for move detection
    let touch_start_x: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
    let touch_start_y: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));

    // Update local value when external signal changes (but only if not actively touching)
    Effect::new(move |_| {
        if !is_activated.get() && !is_pending.get() {
            set_local_value.set(value.get());
        }
    });

    // Wrap on_change in Rc for use in closures
    let on_change = Rc::new(on_change);
    let on_change_touch = on_change.clone();
    let on_change_input = on_change.clone();

    // Touch start: begin activation countdown
    let timeout_handle_start = timeout_handle.clone();
    let touch_start_x_start = touch_start_x.clone();
    let touch_start_y_start = touch_start_y.clone();
    let handle_touchstart = move |ev: TouchEvent| {
        // Store initial touch position
        if let Some(touch) = ev.touches().get(0) {
            *touch_start_x_start.borrow_mut() = Some(touch.client_x() as f64);
            *touch_start_y_start.borrow_mut() = Some(touch.client_y() as f64);
        }

        set_is_pending.set(true);

        // Start activation timeout
        let timeout = gloo_timers::callback::Timeout::new(ACTIVATION_DELAY_MS, move || {
            set_is_activated.set(true);
            set_is_pending.set(false);

            // Haptic feedback on activation
            if let Some(window) = web_sys::window() {
                if let Ok(navigator) = window.navigator().dyn_into::<web_sys::Navigator>() {
                    // Try to vibrate (will be ignored on unsupported devices)
                    let _ = navigator.vibrate_with_duration(50);
                }
            }
        });

        *timeout_handle_start.borrow_mut() = Some(timeout);
    };

    // Touch move: only update value if activated
    let timeout_handle_move = timeout_handle.clone();
    let touch_start_x_move = touch_start_x.clone();
    let touch_start_y_move = touch_start_y.clone();
    let handle_touchmove = move |ev: TouchEvent| {
        if let Some(touch) = ev.touches().get(0) {
            let current_x = touch.client_x() as f64;
            let current_y = touch.client_y() as f64;

            // Check if movement is mostly vertical (scrolling intent)
            if let (Some(start_x), Some(start_y)) =
                (*touch_start_x_move.borrow(), *touch_start_y_move.borrow())
            {
                let dx = (current_x - start_x).abs();
                let dy = (current_y - start_y).abs();

                // If vertical movement exceeds horizontal significantly, cancel activation
                // This allows the page to scroll normally
                if dy > dx + 10.0 && !is_activated.get() {
                    // Cancel pending activation
                    *timeout_handle_move.borrow_mut() = None;
                    set_is_pending.set(false);
                    return;
                }
            }
        }

        if !is_activated.get() {
            // Not activated yet - don't capture the touch, let scroll happen
            return;
        }

        // Fader is activated - prevent scroll and handle input
        ev.prevent_default();

        if let Some(touch) = ev.touches().get(0) {
            if let Some(target) = ev.target() {
                if let Ok(input) = target.dyn_into::<HtmlInputElement>() {
                    let rect = input.get_bounding_client_rect();
                    let x = touch.client_x() as f64 - rect.left();
                    let ratio = (x / rect.width()).clamp(0.0, 1.0);
                    let new_value = min + (ratio as f32) * (max - min);
                    set_local_value.set(new_value);
                    on_change_touch(new_value);
                }
            }
        }
    };

    // Touch end: reset state
    let timeout_handle_end = timeout_handle.clone();
    let handle_touchend = move |_ev: TouchEvent| {
        // Cancel pending activation timeout
        *timeout_handle_end.borrow_mut() = None;
        set_is_pending.set(false);
        set_is_activated.set(false);
        *touch_start_x.borrow_mut() = None;
        *touch_start_y.borrow_mut() = None;
    };

    // Mouse/desktop input: immediate response (no delay needed)
    let handle_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input = target.dyn_into::<HtmlInputElement>().unwrap();
        let new_value: f32 = input.value().parse().unwrap_or(0.0);
        set_local_value.set(new_value);
        on_change_input(new_value);
    };

    view! {
        <div class="fader-container">
            <input
                type="range"
                class=move || {
                    let mut classes = vec!["fader"];
                    if is_pending.get() { classes.push("activating"); }
                    if is_activated.get() { classes.push("active"); }
                    classes.join(" ")
                }
                min=min
                max=max
                step="0.5"
                prop:value=move || local_value.get()
                on:input=handle_input
                on:touchstart=handle_touchstart
                on:touchmove=handle_touchmove
                on:touchend=handle_touchend
                on:touchcancel=move |_| {
                    *timeout_handle.borrow_mut() = None;
                    set_is_pending.set(false);
                    set_is_activated.set(false);
                }
            />
        </div>
    }
}
