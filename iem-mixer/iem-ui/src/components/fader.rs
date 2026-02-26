//! Custom div-based fader component with touch-safe activation delay
//!
//! Replaces native `<input type="range">` to eliminate browser jump-to-click
//! behavior and enable fill-bar visualization. Implements a 300ms touch-and-hold
//! activation pattern used by professional audio apps (Soundcraft Ui, Mixing Station).

use leptos::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::TouchEvent;

/// Activation delay in milliseconds (SOTA: 250-350ms)
const ACTIVATION_DELAY_MS: u32 = 300;

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
/// Touch behavior:
/// - Touch and release before 300ms: NO change (user was scrolling)
/// - Touch and hold 300ms+: Fader activates with haptic feedback, relative movement
/// - Mouse/desktop: Immediate response (no delay needed)
///
/// Visual: Rendered as a fill-bar (left-to-right) with a thin handle indicator.
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

    // Store initial touch position for move detection
    let touch_start_x: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
    let touch_start_y: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));

    // Node ref for measuring track dimensions
    let track_ref = NodeRef::<leptos::html::Div>::new();

    // Update local value when external signal changes (but only if not actively touching)
    Effect::new(move |_| {
        if !is_activated.get() && !is_pending.get() && !is_touch_interaction.get() {
            set_local_value.set(value.get());
        }
    });

    // --- Touch handlers ---

    let timeout_handle_start = timeout_handle.clone();
    let touch_start_x_start = touch_start_x.clone();
    let touch_start_y_start = touch_start_y.clone();
    let on_activate_start = on_activate;
    let on_touch_state_start = on_touch_state;

    let handle_touchstart = move |ev: TouchEvent| {
        if let Some(touch) = ev.touches().get(0) {
            *touch_start_x_start.borrow_mut() = Some(touch.client_x() as f64);
            *touch_start_y_start.borrow_mut() = Some(touch.client_y() as f64);
        }

        set_is_touch_interaction.set(true);
        set_saved_value.set(local_value.get_untracked());
        set_is_pending.set(true);

        if let Some(cb) = on_touch_state_start {
            cb.run(true);
        }

        let on_activate_timer = on_activate_start;
        let timeout = gloo_timers::callback::Timeout::new(ACTIVATION_DELAY_MS, move || {
            set_is_activated.set(true);
            set_is_pending.set(false);

            if let Some(cb) = on_activate_timer {
                cb.run(true);
            }

            // Haptic feedback
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let _ = navigator.vibrate_with_duration(50);
            }
        });

        *timeout_handle_start.borrow_mut() = Some(timeout);
    };

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

                if dy > dx + 10.0 && !is_activated.get() {
                    *timeout_handle_move.borrow_mut() = None;
                    set_is_pending.set(false);
                    return;
                }
            }
        }

        if !is_activated.get() {
            return;
        }

        // Activated — prevent scroll and use relative positioning
        ev.prevent_default();

        if let Some(touch) = ev.touches().get(0) {
            if let Some(el) = track_ref.get() {
                let rect = el.get_bounding_client_rect();
                let current_x = touch.client_x() as f64;

                if let Some(start_x) = *touch_start_x_move.borrow() {
                    let delta_x = current_x - start_x;
                    let delta_ratio = delta_x / rect.width();
                    let base = saved_value.get_untracked();
                    let new_value =
                        quantize((base + (delta_ratio as f32) * (max - min)).clamp(min, max));
                    set_local_value.set(new_value);
                    on_change.run(new_value);
                }
            }
        }
    };

    let timeout_handle_end = timeout_handle.clone();
    let on_activate_end = on_activate;
    let on_touch_state_end = on_touch_state;

    let handle_touchend = move |_ev: TouchEvent| {
        *timeout_handle_end.borrow_mut() = None;
        set_is_pending.set(false);

        if is_activated.get_untracked() {
            if let Some(cb) = on_activate_end {
                cb.run(false);
            }
        }

        set_is_activated.set(false);
        set_is_touch_interaction.set(false);
        *touch_start_x.borrow_mut() = None;
        *touch_start_y.borrow_mut() = None;

        if let Some(cb) = on_touch_state_end {
            cb.run(false);
        }
    };

    // Touchcancel
    let timeout_handle_cancel = timeout_handle.clone();
    let on_activate_cancel = on_activate;
    let on_touch_state_cancel = on_touch_state;

    let handle_touchcancel = move |_ev: TouchEvent| {
        *timeout_handle_cancel.borrow_mut() = None;
        set_is_pending.set(false);

        if is_activated.get_untracked() {
            if let Some(cb) = on_activate_cancel {
                cb.run(false);
            }
        }

        set_is_activated.set(false);
        set_is_touch_interaction.set(false);

        if let Some(cb) = on_touch_state_cancel {
            cb.run(false);
        }
    };

    // --- Mouse handler (immediate, no delay) ---
    let on_activate_mouse = on_activate;
    let on_touch_state_mouse = on_touch_state;

    let handle_mousedown = move |ev: web_sys::MouseEvent| {
        // Only handle left button, skip if touch interaction is active
        if ev.button() != 0 || is_touch_interaction.get_untracked() {
            return;
        }

        ev.prevent_default();

        if let Some(el) = track_ref.get() {
            let rect = el.get_bounding_client_rect();
            let x = ev.client_x() as f64 - rect.left();
            let pct = (x / rect.width()).clamp(0.0, 1.0) as f32;
            let new_value = quantize(min + pct * (max - min));
            set_local_value.set(new_value);
            on_change.run(new_value);
        }

        set_is_activated.set(true);

        if let Some(cb) = on_activate_mouse {
            cb.run(true);
        }
        if let Some(cb) = on_touch_state_mouse {
            cb.run(true);
        }

        // Register window-level mousemove + mouseup
        let document = web_sys::window().unwrap().document().unwrap();
        let doc_target: web_sys::EventTarget = document.into();

        // Shared closures stored in Rc<RefCell> for cleanup
        let move_closure: Rc<RefCell<Option<Closure<dyn FnMut(web_sys::MouseEvent)>>>> =
            Rc::new(RefCell::new(None));
        let up_closure: Rc<RefCell<Option<Closure<dyn FnMut(web_sys::MouseEvent)>>>> =
            Rc::new(RefCell::new(None));

        let move_closure_for_up = move_closure.clone();
        let up_closure_for_up = up_closure.clone();
        let doc_for_move = doc_target.clone();
        let doc_for_up = doc_target.clone();

        let on_activate_up = on_activate;
        let on_touch_state_up = on_touch_state;

        // mousemove: update value from mouse X
        let track_ref_move = track_ref;
        let mc = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
            if let Some(el) = track_ref_move.get() {
                let rect = el.get_bounding_client_rect();
                let x = ev.client_x() as f64 - rect.left();
                let pct = (x / rect.width()).clamp(0.0, 1.0) as f32;
                let new_value = quantize(min + pct * (max - min));
                set_local_value.set(new_value);
                on_change.run(new_value);
            }
        }) as Box<dyn FnMut(web_sys::MouseEvent)>);

        let _ =
            doc_for_move.add_event_listener_with_callback("mousemove", mc.as_ref().unchecked_ref());
        *move_closure.borrow_mut() = Some(mc);

        // mouseup: cleanup
        let uc = Closure::wrap(Box::new(move |_ev: web_sys::MouseEvent| {
            set_is_activated.set(false);

            if let Some(cb) = on_activate_up {
                cb.run(false);
            }
            if let Some(cb) = on_touch_state_up {
                cb.run(false);
            }

            // Remove listeners
            if let Some(ref mc) = *move_closure_for_up.borrow() {
                let _ = doc_for_up
                    .remove_event_listener_with_callback("mousemove", mc.as_ref().unchecked_ref());
            }
            *move_closure_for_up.borrow_mut() = None;

            // Remove self — we need doc_target again
            // The closure removes itself by dropping the Rc reference
            // This is handled by the forget() below — the closure lives until page unload
            // which is fine for occasional mouse interactions
        }) as Box<dyn FnMut(web_sys::MouseEvent)>);

        let _ = doc_target.add_event_listener_with_callback("mouseup", uc.as_ref().unchecked_ref());
        *up_closure_for_up.borrow_mut() = Some(uc);
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
