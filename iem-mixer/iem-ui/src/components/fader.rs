//! Custom div-based fader component with touch-safe activation delay
//!
//! Replaces native `<input type="range">` to eliminate browser jump-to-click
//! behavior and enable fill-bar visualization. Implements a 300ms activation
//! pattern for BOTH touch AND mouse — ALL movement is relative-only.
//! No absolute positioning jumps on any platform.

use leptos::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::TouchEvent;

/// Activation delay in milliseconds (SOTA: 250-350ms)
const ACTIVATION_DELAY_MS: u32 = 300;

/// Time window to ignore synthesized mouse events after touch (ms)
const TOUCH_MOUSE_GUARD_MS: f64 = 500.0;

/// Convert a dB value to a percentage position on the fader track
fn value_to_percent(value: f32, min: f32, max: f32) -> f32 {
    ((value - min) / (max - min) * 100.0).clamp(0.0, 100.0)
}

/// Quantize to 0.5 dB steps
fn quantize(value: f32) -> f32 {
    (value * 2.0).round() / 2.0
}

/// Horizontal fader component with touch-safe activation
///
/// Both touch and mouse use the same interaction model:
/// - Press and release before 300ms: NO change (prevents accidental jumps)
/// - Press and hold 300ms+: Fader activates with visual feedback, relative movement
/// - All movement is relative — fader never jumps to click/tap position
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
    on_change: Callback<f32>,
    /// Fires true on activation (300ms hold), false on release
    #[prop(optional)]
    on_activate: Option<Callback<bool>>,
    /// Fires true on touchstart/mousedown, false on touchend/mouseup
    #[prop(optional)]
    on_touch_state: Option<Callback<bool>>,
) -> impl IntoView {
    let (local_value, set_local_value) = signal(value.get_untracked());
    let (is_activated, set_is_activated) = signal(false);
    let (is_pending, set_is_pending) = signal(false);
    let (is_touch_interaction, set_is_touch_interaction) = signal(false);
    let (saved_value, set_saved_value) = signal(0.0f32);

    // Store the timeout handle so we can cancel it
    let timeout_handle: Rc<RefCell<Option<gloo_timers::callback::Timeout>>> =
        Rc::new(RefCell::new(None));

    // Original touch position for vertical-scroll detection
    let touch_start_x: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
    let touch_start_y: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));

    // Latest pointer position — baseline for relative movement after activation.
    // Updated continuously during pre-activation to eliminate accumulated drift.
    let move_base_x: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));

    // Timestamp of last touch event — guards against synthesized mouse events
    let last_touch_time: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));

    // Node ref for measuring track dimensions
    let track_ref = NodeRef::<leptos::html::Div>::new();

    // Update local value when external signal changes (but only if not actively touching)
    Effect::new(move |_| {
        if !is_activated.get() && !is_pending.get() && !is_touch_interaction.get() {
            set_local_value.set(value.get());
        }
    });

    // --- Rc clones for each closure ---

    let timeout_handle_ts = timeout_handle.clone();
    let timeout_handle_tm = timeout_handle.clone();
    let timeout_handle_te = timeout_handle.clone();
    let timeout_handle_tc = timeout_handle.clone();
    let timeout_handle_md = timeout_handle; // mousedown (cloned inside for inner closures)

    let touch_start_x_ts = touch_start_x.clone();
    let touch_start_x_tm = touch_start_x.clone();
    // touch_start_x consumed by touchend

    let touch_start_y_ts = touch_start_y.clone();
    let touch_start_y_tm = touch_start_y.clone();
    // touch_start_y consumed by touchend

    let move_base_x_ts = move_base_x.clone();
    let move_base_x_tm = move_base_x.clone();
    let move_base_x_te = move_base_x.clone();
    let move_base_x_md = move_base_x; // mousedown (cloned inside for inner closures)

    let last_touch_time_ts = last_touch_time.clone();
    let last_touch_time_te = last_touch_time.clone();
    let last_touch_time_tc = last_touch_time.clone();
    let last_touch_time_md = last_touch_time; // mousedown (last user)

    // --- Touch handlers ---

    let handle_touchstart = move |ev: TouchEvent| {
        *last_touch_time_ts.borrow_mut() = js_sys::Date::now();

        if let Some(touch) = ev.touches().get(0) {
            let x = touch.client_x() as f64;
            *touch_start_x_ts.borrow_mut() = Some(x);
            *touch_start_y_ts.borrow_mut() = Some(touch.client_y() as f64);
            *move_base_x_ts.borrow_mut() = Some(x);
        }

        set_is_touch_interaction.set(true);
        set_saved_value.set(local_value.get_untracked());
        set_is_pending.set(true);

        if let Some(cb) = on_touch_state {
            cb.run(true);
        }

        let timeout = gloo_timers::callback::Timeout::new(ACTIVATION_DELAY_MS, move || {
            set_is_activated.set(true);
            set_is_pending.set(false);

            if let Some(cb) = on_activate {
                cb.run(true);
            }

            // Haptic feedback
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let _ = navigator.vibrate_with_duration(50);
            }
        });

        *timeout_handle_ts.borrow_mut() = Some(timeout);
    };

    let handle_touchmove = move |ev: TouchEvent| {
        if let Some(touch) = ev.touches().get(0) {
            let current_x = touch.client_x() as f64;
            let current_y = touch.client_y() as f64;

            // Check if movement is mostly vertical (scrolling intent)
            if let (Some(start_x), Some(start_y)) =
                (*touch_start_x_tm.borrow(), *touch_start_y_tm.borrow())
            {
                let dx = (current_x - start_x).abs();
                let dy = (current_y - start_y).abs();

                if dy > dx + 10.0 && !is_activated.get() {
                    *timeout_handle_tm.borrow_mut() = None;
                    set_is_pending.set(false);
                    return;
                }
            }

            if !is_activated.get() {
                // Pre-activation: track latest position for clean baseline
                *move_base_x_tm.borrow_mut() = Some(current_x);
                return;
            }

            // Activated — prevent scroll and use relative positioning
            ev.prevent_default();

            if let Some(el) = track_ref.get() {
                let rect = el.get_bounding_client_rect();

                // Extract value BEFORE if let to release immutable borrow
                // (avoids RefCell panic when we borrow_mut inside the block)
                let base_x_opt = *move_base_x_tm.borrow();
                if let Some(base_x) = base_x_opt {
                    let delta_x = current_x - base_x;
                    let delta_ratio = delta_x / rect.width();
                    let base = saved_value.get_untracked();
                    let new_value =
                        quantize((base + (delta_ratio as f32) * (max - min)).clamp(min, max));
                    set_local_value.set(new_value);
                    on_change.run(new_value);
                    // Update base for next incremental move
                    *move_base_x_tm.borrow_mut() = Some(current_x);
                    set_saved_value.set(new_value);
                }
            }
        }
    };

    let handle_touchend = move |_ev: TouchEvent| {
        *last_touch_time_te.borrow_mut() = js_sys::Date::now();
        *timeout_handle_te.borrow_mut() = None;
        set_is_pending.set(false);

        if is_activated.get_untracked() {
            if let Some(cb) = on_activate {
                cb.run(false);
            }
        }

        set_is_activated.set(false);
        set_is_touch_interaction.set(false);
        *touch_start_x.borrow_mut() = None;
        *touch_start_y.borrow_mut() = None;
        *move_base_x_te.borrow_mut() = None;

        if let Some(cb) = on_touch_state {
            cb.run(false);
        }
    };

    let handle_touchcancel = move |_ev: TouchEvent| {
        *last_touch_time_tc.borrow_mut() = js_sys::Date::now();
        *timeout_handle_tc.borrow_mut() = None;
        set_is_pending.set(false);

        if is_activated.get_untracked() {
            if let Some(cb) = on_activate {
                cb.run(false);
            }
        }

        set_is_activated.set(false);
        set_is_touch_interaction.set(false);

        if let Some(cb) = on_touch_state {
            cb.run(false);
        }
    };

    // --- Mouse handler (300ms activation, relative movement — same as touch) ---

    let handle_mousedown = move |ev: web_sys::MouseEvent| {
        // Only handle left button, skip if touch interaction is active
        if ev.button() != 0 || is_touch_interaction.get_untracked() {
            return;
        }

        // Guard against synthesized mouse events from touch
        if js_sys::Date::now() - *last_touch_time_md.borrow() < TOUCH_MOUSE_GUARD_MS {
            return;
        }

        ev.prevent_default();

        // Save current value and mouse position — NO JUMP, relative only
        set_saved_value.set(local_value.get_untracked());
        *move_base_x_md.borrow_mut() = Some(ev.client_x() as f64);
        set_is_pending.set(true);

        if let Some(cb) = on_touch_state {
            cb.run(true);
        }

        // Start 300ms activation timer (same as touch)
        let timeout = gloo_timers::callback::Timeout::new(ACTIVATION_DELAY_MS, move || {
            set_is_activated.set(true);
            set_is_pending.set(false);

            if let Some(cb) = on_activate {
                cb.run(true);
            }
        });

        *timeout_handle_md.borrow_mut() = Some(timeout);

        // Register window-level mousemove + mouseup for drag tracking
        let document = web_sys::window().unwrap().document().unwrap();
        let doc_target: web_sys::EventTarget = document.clone().into();

        // Clone Rcs inside body for inner closures (avoids moving out of FnMut)
        let move_base_x_mm = move_base_x_md.clone();
        let move_base_x_mu = move_base_x_md.clone();
        let timeout_handle_mu = timeout_handle_md.clone();

        // mousemove: RELATIVE movement from move_base_x (no absolute positioning)
        let track_ref_move = track_ref;
        let mc = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
            let current_x = ev.client_x() as f64;

            if !is_activated.get() {
                // Pre-activation: track position for clean baseline
                *move_base_x_mm.borrow_mut() = Some(current_x);
                return;
            }

            // Activated: relative delta from move_base_x
            if let Some(el) = track_ref_move.get() {
                // Extract value BEFORE if let to release immutable borrow
                // (avoids RefCell panic when we borrow_mut inside the block)
                let base_x_opt = *move_base_x_mm.borrow();
                if let Some(base_x) = base_x_opt {
                    let rect = el.get_bounding_client_rect();
                    let delta_x = current_x - base_x;
                    let delta_ratio = delta_x / rect.width();
                    let base = saved_value.get_untracked();
                    let new_value =
                        quantize((base + (delta_ratio as f32) * (max - min)).clamp(min, max));
                    set_local_value.set(new_value);
                    on_change.run(new_value);
                    // Update base for next incremental move
                    *move_base_x_mm.borrow_mut() = Some(current_x);
                    set_saved_value.set(new_value);
                }
            }
        }) as Box<dyn FnMut(web_sys::MouseEvent)>);

        let _ =
            doc_target.add_event_listener_with_callback("mousemove", mc.as_ref().unchecked_ref());
        mc.forget(); // Prevent closure from being dropped when mousedown returns

        // mouseup: cleanup all state (listeners leak but this is standard wasm-bindgen pattern)
        let uc = Closure::wrap(Box::new(move |_ev: web_sys::MouseEvent| {
            // Cancel activation timer if still pending
            *timeout_handle_mu.borrow_mut() = None;
            set_is_pending.set(false);

            if is_activated.get_untracked() {
                if let Some(cb) = on_activate {
                    cb.run(false);
                }
            }

            set_is_activated.set(false);
            *move_base_x_mu.borrow_mut() = None;

            if let Some(cb) = on_touch_state {
                cb.run(false);
            }
        }) as Box<dyn FnMut(web_sys::MouseEvent)>);

        let _ = doc_target.add_event_listener_with_callback("mouseup", uc.as_ref().unchecked_ref());
        uc.forget(); // Prevent closure from being dropped when mousedown returns
    };

    // Compute percentage for rendering
    let pct = move || value_to_percent(local_value.get(), min, max);

    view! {
        <div
            node_ref=track_ref
            class=move || {
                let mut classes = vec!["fader-track"];
                if is_pending.get() { classes.push("activating"); }
                if is_activated.get() { classes.push("active"); }
                classes.join(" ")
            }
            on:touchstart=handle_touchstart
            on:touchmove=handle_touchmove
            on:touchend=handle_touchend
            on:touchcancel=handle_touchcancel
            on:mousedown=handle_mousedown
        >
            <div class="fader-fill" style=move || format!("width:{}%", pct()) />
            <div class="fader-handle" style=move || format!("left:{}%", pct()) />
        </div>
    }
}
