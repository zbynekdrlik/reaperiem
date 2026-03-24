//! Full-screen parametric EQ modal with SVG frequency response curve
//!
//! Loads EQ state on-demand from REAPER via GetEqParams, displays draggable
//! band points on a log-frequency curve, and sends SetEqBand on slider changes.
//!
//! v1.104.0: Fix snap-back bug — EqSlider maintains local reactive state so
//! parent re-renders don't destroy active drag gestures. Each band card owns
//! its own signals, and on_change only sends WebSocket (no set_bands.set()).
//!
//! v1.107.0: Display values from REAPER (fh/gd/bo), double-tap to default,
//! per-band on/off toggle and reset buttons.

use leptos::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// Activation delay in milliseconds (matches fader.rs pattern)
const ACTIVATION_DELAY_MS: u32 = 150;

/// Maximum time between taps for double-tap detection (ms)
const DOUBLE_TAP_MS: f64 = 300.0;

/// EQ band data (mirrors iem_core::EqBand for frontend use)
#[derive(Debug, Clone, PartialEq)]
pub struct EqBandState {
    pub band_type: String,
    pub freq_hz: f32,
    pub gain_db: f32,
    pub bw: f32,
    pub freq_norm: f32,
    pub gain_norm: f32,
    pub bw_norm: f32,
}

/// Band type colors for visual distinction on the curve
fn band_color(band_type: &str) -> &'static str {
    match band_type {
        "lowshelf" => "#f4d35e",
        "highshelf" => "#ff6b6b",
        "highpass" => "#4ecdc4",
        "lowpass" => "#a06cd5",
        "notch" => "#ff9f43",
        _ => "#4ecdc4", // "band" = default accent
    }
}

/// Convert normalized frequency (0-1) to Hz (ReaEQ log mapping: 20 Hz to 20 kHz)
fn norm_to_freq_hz(norm: f32) -> f32 {
    20.0 * 1000.0_f32.powf(norm)
}

/// Approximate normalized gain (0-1) to dB for drag feedback only.
/// Server provides accurate REAPER-formatted values; this is used during drag
/// when server values are not yet available. Uses cubic curve fitted to verified
/// data points: norm=0.183911 -> -2.7 dB, norm=0.25 -> 0 dB, norm=0.288511 -> +1.2 dB.
fn norm_to_gain_db(norm: f32) -> f32 {
    let x = norm - 0.25;
    // Cubic approximation: asymmetric curve matching verified REAPER data
    if x >= 0.0 {
        x * 32.0 + x * x * 40.0
    } else {
        x * 32.0 - x * x * 40.0
    }
}

/// Convert normalized bandwidth (0-1) to octaves
fn norm_to_bw(norm: f32) -> f32 {
    0.01 + norm * 3.99
}

/// Convert frequency in Hz to SVG x position (20Hz-20kHz log scale)
fn freq_to_x(freq_hz: f32, width: f32) -> f32 {
    let log_min = 20.0_f32.ln();
    let log_max = 20000.0_f32.ln();
    let log_freq = freq_hz.clamp(20.0, 20000.0).ln();
    ((log_freq - log_min) / (log_max - log_min)) * width
}

/// Convert gain in dB to SVG y position (-24 to +24 dB)
fn gain_to_y(gain_db: f32, height: f32) -> f32 {
    let clamped = gain_db.clamp(-24.0, 24.0);
    // +24 at top (y=0), -24 at bottom (y=height)
    ((24.0 - clamped) / 48.0) * height
}

/// Generate the frequency response curve path as SVG "d" attribute.
/// Uses a simplified approximation: each band contributes gain in a bell/shelf shape.
fn generate_curve_path(bands: &[EqBandState], width: f32, height: f32) -> String {
    let num_points = 200;
    let log_min = 20.0_f32.ln();
    let log_max = 20000.0_f32.ln();
    let mut path = String::with_capacity(num_points * 20);

    for i in 0..=num_points {
        let x = (i as f32 / num_points as f32) * width;
        let log_freq = log_min + (i as f32 / num_points as f32) * (log_max - log_min);
        let freq = log_freq.exp();

        // Sum contributions from all bands
        let mut total_gain = 0.0_f32;
        for band in bands {
            if band.gain_db.abs() < 0.01 && band.band_type == "band" {
                continue; // Skip flat bands
            }
            let band_contribution = compute_band_gain(freq, band);
            total_gain += band_contribution;
        }

        let y = gain_to_y(total_gain, height);

        if i == 0 {
            path.push_str(&format!("M{:.1},{:.1}", x, y));
        } else {
            path.push_str(&format!(" L{:.1},{:.1}", x, y));
        }
    }
    path
}

/// Compute the gain contribution of a single band at a given frequency.
/// Simplified model using octave-based bandwidth.
fn compute_band_gain(freq: f32, band: &EqBandState) -> f32 {
    let log_ratio = (freq / band.freq_hz).ln() / 2.0_f32.ln(); // In octaves

    match band.band_type.as_str() {
        "highpass" => {
            // High-pass: sharp roll-off below cutoff
            if freq < band.freq_hz {
                let octaves_below = (band.freq_hz / freq).ln() / 2.0_f32.ln();
                -octaves_below * 12.0 // -12dB/oct
            } else {
                0.0
            }
        }
        "lowpass" => {
            if freq > band.freq_hz {
                let octaves_above = (freq / band.freq_hz).ln() / 2.0_f32.ln();
                -octaves_above * 12.0
            } else {
                0.0
            }
        }
        "lowshelf" => {
            // Low shelf: full gain below freq, rolls off above
            let transition = (-log_ratio / band.bw.max(0.1)).exp();
            band.gain_db * transition / (1.0 + transition)
                + band.gain_db / (1.0 + (1.0 / transition))
                - band.gain_db
        }
        "highshelf" => {
            let transition = (log_ratio / band.bw.max(0.1)).exp();
            band.gain_db * transition / (1.0 + transition)
                + band.gain_db / (1.0 + (1.0 / transition))
                - band.gain_db
        }
        "notch" => {
            // Narrow dip
            let q = 1.0 / band.bw.max(0.1);
            let x = log_ratio * q;
            -6.0 * (-x * x).exp() // Fixed -6dB notch
        }
        _ => {
            // Parametric bell (band)
            let q = 1.0 / band.bw.max(0.1);
            let x = log_ratio * q;
            band.gain_db * (-x * x).exp()
        }
    }
}

/// Format frequency for display
fn format_freq(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.1}k", hz / 1000.0)
    } else {
        format!("{:.0}", hz)
    }
}

/// Per-band local state signals that survive parent re-renders.
/// These are created once per band when the modal opens and persist until close.
#[derive(Clone)]
struct BandLocalState {
    band_type: String,
    freq_norm: RwSignal<f32>,
    gain_norm: RwSignal<f32>,
    bw_norm: RwSignal<f32>,
    /// REAPER-formatted display values (accurate, loaded from server)
    freq_hz: RwSignal<f32>,
    gain_db: RwSignal<f32>,
    bw_oct: RwSignal<f32>,
    /// Whether this band is enabled (gain != 0 dB)
    enabled: RwSignal<bool>,
    /// Saved gain_norm before disable (for re-enable toggle)
    saved_gain_norm: RwSignal<f32>,
    /// Saved gain_db before disable
    saved_gain_db: RwSignal<f32>,
    /// Initial freq_hz loaded from server (for reset)
    initial_freq_hz: f32,
    /// Initial freq_norm loaded from server (for reset)
    initial_freq_norm: f32,
}

/// Full-screen EQ modal component
#[component]
pub fn EQModal(
    /// Track index to show EQ for
    track_index: usize,
    /// Track name for the header
    track_name: String,
    /// EQ bands data from server (synced to local signals when not dragging)
    bands: ReadSignal<Vec<EqBandState>>,
    /// Whether EQ data is loading
    loading: ReadSignal<bool>,
    /// Callback when a band parameter changes (band_index, param_name, normalized_value)
    on_param_change: Callback<(u8, String, f32)>,
    /// Callback to close the modal
    on_close: Callback<()>,
) -> impl IntoView {
    let _track_index = track_index; // Used for future SVG interaction
    let track_name = StoredValue::new(track_name);

    // SVG dimensions
    let svg_width = 800.0_f32;
    let svg_height = 300.0_f32;

    // Generate the 0dB reference line y position
    let zero_db_y = gain_to_y(0.0, svg_height);

    // Grid lines for frequency axis (log scale)
    let freq_grid_lines: Vec<(f32, String)> = [
        20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
    ]
    .iter()
    .map(|&f| (freq_to_x(f, svg_width), format_freq(f)))
    .collect();

    // Grid lines for gain axis
    let gain_grid_lines: Vec<(f32, String)> =
        [-24.0, -18.0, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0, 24.0]
            .iter()
            .map(|&g| (gain_to_y(g, svg_height), format!("{:+.0}", g)))
            .collect();

    // Per-band local signals stored ONCE — never replaced, so DOM stays stable.
    // StoredValue is NOT reactive: reading it does not subscribe, so the band card
    // DOM created from it is rendered exactly once and never torn down.
    let stored_locals: StoredValue<Vec<BandLocalState>> = StoredValue::new(Vec::new());

    // Gate signal: flips to true ONCE when bands data first arrives.
    // The <Show> component renders band cards when this becomes true, but
    // since stored_locals is non-reactive, the cards are created once and persist.
    let local_state_created = RwSignal::new(false);

    // Track whether any slider is currently being dragged (guards against server echo)
    let any_dragging = RwSignal::new(false);

    // Explicit trigger for curve + display updates. Incremented AFTER signal writes complete.
    // The Memo and display closures subscribe to THIS (not to individual band signals),
    // preventing recursive closure invocation that causes WASM panics.
    let curve_trigger = RwSignal::new(0u32);

    // Sync from parent bands signal into local signals.
    // First arrival: populate stored_locals and flip the gate.
    // Subsequent: update existing RwSignals (no DOM destruction).
    Effect::new(move |_| {
        let parent = bands.get();
        if parent.is_empty() {
            return;
        }

        if !local_state_created.get_untracked() {
            // First time: create local signals and store them
            let locals: Vec<BandLocalState> = parent
                .iter()
                .map(|b| {
                    let is_enabled = b.gain_db.abs() > 0.05;
                    BandLocalState {
                        band_type: b.band_type.clone(),
                        freq_norm: RwSignal::new(b.freq_norm),
                        gain_norm: RwSignal::new(b.gain_norm),
                        bw_norm: RwSignal::new(b.bw_norm),
                        freq_hz: RwSignal::new(b.freq_hz),
                        gain_db: RwSignal::new(b.gain_db),
                        bw_oct: RwSignal::new(b.bw),
                        enabled: RwSignal::new(is_enabled),
                        saved_gain_norm: RwSignal::new(b.gain_norm),
                        saved_gain_db: RwSignal::new(b.gain_db),
                        initial_freq_hz: b.freq_hz,
                        initial_freq_norm: b.freq_norm,
                    }
                })
                .collect();
            stored_locals.set_value(locals);
            // Defer gate flip to next microtask — Effect must complete before
            // <Show> renders its body (which reads the RwSignals we just created).
            // Without deferral, Leptos detects recursive closure invocation → WASM panic.
            wasm_bindgen_futures::spawn_local(async move {
                local_state_created.set(true);
            });
        } else if !any_dragging.get_untracked() {
            // Subsequent: sync values then trigger display update
            // Defer to microtask — updating curve_trigger inside this Effect
            // would cause recursive notification (Effect → set → trigger → Memo)
            let locals = stored_locals.get_value();
            for (local, parent_band) in locals.iter().zip(parent.iter()) {
                local.freq_norm.set(parent_band.freq_norm);
                local.gain_norm.set(parent_band.gain_norm);
                local.bw_norm.set(parent_band.bw_norm);
                local.freq_hz.set(parent_band.freq_hz);
                local.gain_db.set(parent_band.gain_db);
                local.bw_oct.set(parent_band.bw);
            }
            curve_trigger.update(|n| *n += 1);
        }
    });

    view! {
        <div class="eq-overlay" on:click=move |_| on_close.run(())>
            <div class="eq-modal" on:click=move |e: web_sys::MouseEvent| e.stop_propagation()>
                // Header
                <div class="eq-header">
                    <span class="eq-title">"EQ: " {move || track_name.get_value()}</span>
                    <button class="eq-close-btn" on:click=move |_| on_close.run(())>
                        "\u{2715}"
                    </button>
                </div>

                // Loading indicator
                <Show when=move || loading.get() fallback=|| ()>
                    <div class="eq-loading">"Loading EQ..."</div>
                </Show>

                // No EQ message
                <Show when=move || !loading.get() && !local_state_created.get() fallback=|| ()>
                    <div class="eq-no-eq">"No ReaEQ found on this track"</div>
                </Show>

                // SVG Curve display + band controls
                // This <Show> flips ONCE when bands data arrives. The content inside
                // is rendered once and never torn down (stored_locals is non-reactive).
                <Show when=move || local_state_created.get() fallback=|| ()>
                    <div class="eq-curve-container">
                        <svg
                            viewBox=format!("0 0 {} {}", svg_width, svg_height)
                            class="eq-curve-svg"
                            preserveAspectRatio="none"
                        >
                            // Background grid - frequency lines
                            {freq_grid_lines.iter().map(|(x, label)| {
                                let x = *x;
                                let label = label.clone();
                                view! {
                                    <line
                                        x1=x x2=x y1=0 y2=svg_height
                                        stroke="rgba(255,255,255,0.08)" stroke-width="1"
                                    />
                                    <text x=x y=svg_height - 4.0 fill="rgba(255,255,255,0.3)"
                                        font-size="10" text-anchor="middle">
                                        {label}
                                    </text>
                                }
                            }).collect::<Vec<_>>()}

                            // Background grid - gain lines
                            {gain_grid_lines.iter().map(|(y, label)| {
                                let y = *y;
                                let label = label.clone();
                                let opacity = if label == "+0" { "0.25" } else { "0.08" };
                                let width = if label == "+0" { "1.5" } else { "1" };
                                view! {
                                    <line
                                        x1=0 x2=svg_width y1=y y2=y
                                        stroke=format!("rgba(255,255,255,{})", opacity)
                                        stroke-width=width
                                    />
                                    <text x=4 y=y - 2.0 fill="rgba(255,255,255,0.3)"
                                        font-size="10">
                                        {label}
                                    </text>
                                }
                            }).collect::<Vec<_>>()}

                            // 0dB reference line (brighter)
                            <line
                                x1=0 x2=svg_width y1=zero_db_y y2=zero_db_y
                                stroke="rgba(255,255,255,0.25)" stroke-width="1.5"
                            />

                            // Frequency response curve — triggered ONLY by curve_trigger (not band signals)
                            // Uses get_untracked() to read band values without subscribing,
                            // preventing recursive closure invocation that kills the Memo.
                            {
                                let curve_memo = Memo::new(move |_| {
                                    curve_trigger.get(); // ONLY subscription
                                    let locals = stored_locals.get_value();
                                    let states: Vec<EqBandState> = locals.iter().map(|l| {
                                        let fn_ = l.freq_norm.get_untracked();
                                        let gn_ = l.gain_norm.get_untracked();
                                        let bn_ = l.bw_norm.get_untracked();
                                        EqBandState {
                                            band_type: l.band_type.clone(),
                                            freq_hz: l.freq_hz.get_untracked(),
                                            gain_db: l.gain_db.get_untracked(),
                                            bw: l.bw_oct.get_untracked(),
                                            freq_norm: fn_, gain_norm: gn_, bw_norm: bn_,
                                        }
                                    }).collect();
                                    generate_curve_path(&states, svg_width, svg_height)
                                });
                                view! {
                                    <path
                                        d=move || curve_memo.get()
                                        fill="none"
                                        stroke="var(--accent)"
                                        stroke-width="2.5"
                                    />
                                    <path
                                        d=move || {
                                            let curve = curve_memo.get();
                                            format!("{} L{:.1},{:.1} L0,{:.1} Z", curve, svg_width, zero_db_y, zero_db_y)
                                        }
                                        fill="rgba(78, 205, 196, 0.08)"
                                    />
                                }
                            }

                            // Band points — stable SVG elements with reactive attributes.
                            // NOT wrapped in {move || ...} to avoid re-creating DOM.
                            {
                                let locals = stored_locals.get_value();
                                locals.iter().enumerate().map(|(i, local)| {
                                    let color = band_color(&local.band_type).to_string();
                                    let freq_hz_sig = local.freq_hz;
                                    let gain_db_sig = local.gain_db;
                                    view! {
                                        <circle
                                            cx=move || { curve_trigger.get(); freq_to_x(freq_hz_sig.get_untracked(), svg_width) }
                                            cy=move || { curve_trigger.get(); gain_to_y(gain_db_sig.get_untracked(), svg_height) }
                                            r="8"
                                            fill=color.clone()
                                            stroke="white" stroke-width="2"
                                            opacity="0.9"
                                        />
                                        <text
                                            x=move || { curve_trigger.get(); freq_to_x(freq_hz_sig.get_untracked(), svg_width) }
                                            y=move || { curve_trigger.get(); gain_to_y(gain_db_sig.get_untracked(), svg_height) - 12.0 }
                                            fill="white" font-size="11" text-anchor="middle"
                                            font-weight="bold"
                                        >
                                            {format!("{}", i + 1)}
                                        </text>
                                    }
                                }).collect::<Vec<_>>()
                            }
                        </svg>
                    </div>

                    // Band controls — rendered ONCE from stored_locals (non-reactive).
                    // NO {move || ...} wrapper means this DOM is created once and never
                    // torn down. Individual RwSignal<f32> values update display text
                    // reactively without destroying the EqSlider components or their
                    // event handler closures.
                    <div class="eq-band-controls">
                        {
                            let locals = stored_locals.get_value();
                            locals.iter().enumerate().map(|(i, local)| {
                                let band_idx = i as u8;
                                let band_type = local.band_type.clone();
                                let color = band_color(&band_type).to_string();

                                // Get the stable local signals for this band
                                let freq_sig = local.freq_norm;
                                let gain_sig = local.gain_norm;
                                let bw_sig = local.bw_norm;
                                let freq_hz_sig = local.freq_hz;
                                let gain_db_sig = local.gain_db;
                                let bw_oct_sig = local.bw_oct;
                                let enabled_sig = local.enabled;
                                let saved_gain_norm_sig = local.saved_gain_norm;
                                let saved_gain_db_sig = local.saved_gain_db;
                                let initial_freq_norm = local.initial_freq_norm;
                                let initial_freq_hz = local.initial_freq_hz;

                                // Throttle WebSocket sends to 50ms intervals per band.
                                // Use RwSignal (Send+Sync) instead of Rc<Cell> for Callback compatibility.
                                let last_send_freq = RwSignal::new(0.0_f64);
                                let last_send_gain = RwSignal::new(0.0_f64);
                                let last_send_bw = RwSignal::new(0.0_f64);

                                view! {
                                    <div class="eq-band-card" style=format!("border-color: {}", color)>
                                        <div class="eq-band-header">
                                            <span class="eq-band-num" style=format!("background: {}", color)>
                                                {i + 1}
                                            </span>
                                            <span class="eq-band-type">{band_type.clone()}</span>
                                            // Toggle on/off button
                                            <button
                                                class=move || {
                                                    if enabled_sig.get() { "eq-band-toggle on" } else { "eq-band-toggle off" }
                                                }
                                                on:click=move |_| {
                                                    if enabled_sig.get_untracked() {
                                                        // Disable: save current gain, set to 0 dB
                                                        saved_gain_norm_sig.set(gain_sig.get_untracked());
                                                        saved_gain_db_sig.set(gain_db_sig.get_untracked());
                                                        gain_sig.set(0.25);
                                                        gain_db_sig.set(0.0);
                                                        enabled_sig.set(false);
                                                        on_param_change.run((band_idx, "gain".to_string(), 0.25));
                                                    } else {
                                                        // Re-enable: restore saved gain
                                                        let saved_norm = saved_gain_norm_sig.get_untracked();
                                                        let saved_db = saved_gain_db_sig.get_untracked();
                                                        gain_sig.set(saved_norm);
                                                        gain_db_sig.set(saved_db);
                                                        enabled_sig.set(true);
                                                        on_param_change.run((band_idx, "gain".to_string(), saved_norm));
                                                    }
                                                    curve_trigger.update(|n| *n += 1);
                                                }
                                            />
                                            // Reset button
                                            <button
                                                class="eq-band-reset"
                                                title="Reset band"
                                                on:click=move |_| {
                                                    // Reset gain to 0 dB
                                                    gain_sig.set(0.25);
                                                    gain_db_sig.set(0.0);
                                                    on_param_change.run((band_idx, "gain".to_string(), 0.25));
                                                    // Reset BW to 0.5
                                                    bw_sig.set(0.5);
                                                    bw_oct_sig.set(norm_to_bw(0.5));
                                                    on_param_change.run((band_idx, "bw".to_string(), 0.5));
                                                    // Reset freq to initial loaded value
                                                    freq_sig.set(initial_freq_norm);
                                                    freq_hz_sig.set(initial_freq_hz);
                                                    on_param_change.run((band_idx, "freq".to_string(), initial_freq_norm));
                                                    // Update enabled state
                                                    enabled_sig.set(false);
                                                    saved_gain_norm_sig.set(0.25);
                                                    saved_gain_db_sig.set(0.0);
                                                    curve_trigger.update(|n| *n += 1);
                                                }
                                            >
                                                "\u{21BA}"
                                            </button>
                                        </div>

                                        // Frequency slider
                                        <div class="eq-param-row">
                                            <label class="eq-param-label">"Freq"</label>
                                            <EqSlider
                                                value=freq_sig.into()
                                                on_change=Callback::new(move |v: f32| {
                                                    // Send WebSocket FIRST (before signal.set which may trigger Memo panic)
                                                    let now = js_sys::Date::now();
                                                    if now - last_send_freq.get_untracked() > 50.0 {
                                                        last_send_freq.set(now);
                                                        on_param_change.run((band_idx, "freq".to_string(), v));
                                                    }
                                                    freq_sig.set(v);
                                                    // Approximate display during drag (server corrects on next open)
                                                    freq_hz_sig.set(norm_to_freq_hz(v));
                                                    curve_trigger.update(|n| *n += 1);
                                                })
                                                on_drag_start=Callback::new(move |_: ()| {
                                                    any_dragging.set(true);
                                                })
                                                on_drag_end=Callback::new(move |_: ()| {
                                                    any_dragging.set(false);
                                                })
                                                css_class="eq-slider-freq"
                                            />
                                            <span class="eq-param-value">
                                                {move || { curve_trigger.get(); format_freq(freq_hz_sig.get_untracked()) }}
                                            </span>
                                        </div>

                                        // Gain slider
                                        <div class="eq-param-row">
                                            <label class="eq-param-label">"Gain"</label>
                                            <EqSlider
                                                value=gain_sig.into()
                                                on_change=Callback::new(move |v: f32| {
                                                    // Send WebSocket FIRST (before signal.set which may trigger Memo panic)
                                                    let now = js_sys::Date::now();
                                                    if now - last_send_gain.get_untracked() > 50.0 {
                                                        last_send_gain.set(now);
                                                        on_param_change.run((band_idx, "gain".to_string(), v));
                                                    }
                                                    gain_sig.set(v);
                                                    // Approximate display during drag
                                                    gain_db_sig.set(norm_to_gain_db(v));
                                                    // Update enabled state
                                                    enabled_sig.set(v != 0.25);
                                                    curve_trigger.update(|n| *n += 1);
                                                })
                                                on_drag_start=Callback::new(move |_: ()| {
                                                    any_dragging.set(true);
                                                })
                                                on_drag_end=Callback::new(move |_: ()| {
                                                    any_dragging.set(false);
                                                })
                                                css_class="eq-slider-gain"
                                                default_value=0.25
                                            />
                                            <span class="eq-param-value">
                                                {move || {
                                                    curve_trigger.get();
                                                    let db = gain_db_sig.get_untracked();
                                                    if db >= 0.0 { format!("+{:.1} dB", db) } else { format!("{:.1} dB", db) }
                                                }}
                                            </span>
                                        </div>

                                        // Bandwidth/Q slider
                                        <div class="eq-param-row">
                                            <label class="eq-param-label">"BW"</label>
                                            <EqSlider
                                                value=bw_sig.into()
                                                on_change=Callback::new(move |v: f32| {
                                                    // Send WebSocket FIRST (before signal.set which may trigger Memo panic)
                                                    let now = js_sys::Date::now();
                                                    if now - last_send_bw.get_untracked() > 50.0 {
                                                        last_send_bw.set(now);
                                                        on_param_change.run((band_idx, "bw".to_string(), v));
                                                    }
                                                    bw_sig.set(v);
                                                    // Approximate display during drag
                                                    bw_oct_sig.set(norm_to_bw(v));
                                                    curve_trigger.update(|n| *n += 1);
                                                })
                                                on_drag_start=Callback::new(move |_: ()| {
                                                    any_dragging.set(true);
                                                })
                                                on_drag_end=Callback::new(move |_: ()| {
                                                    any_dragging.set(false);
                                                })
                                                css_class=""
                                                default_value=0.5
                                            />
                                            <span class="eq-param-value">
                                                {move || { curve_trigger.get(); format!("{:.2} oct", bw_oct_sig.get_untracked()) }}
                                            </span>
                                        </div>
                                    </div>
                                }
                            }).collect::<Vec<_>>()
                        }
                    </div>
                </Show>
            </div>
        </div>
    }
}

/// Touch-safe horizontal slider for EQ parameters.
///
/// Follows the same 150ms activation pattern as the Fader component:
/// - Press and hold 150ms: activates with visual feedback
/// - All movement is relative (never jumps to tap position)
/// - Short taps are ignored (prevents accidental changes while scrolling)
/// - Double-tap resets to default_value (if set, within 300ms)
///
/// v1.104.0: Uses internal `RwSignal<f32>` for display so parent re-renders
/// don't destroy the drag gesture. The `value` prop is a `ReadSignal<f32>`
/// that syncs to the internal signal only when not dragging.
///
/// v1.107.0: Double-tap to default value support.
#[component]
fn EqSlider(
    /// Current normalized value (0-1) from parent signal
    value: Signal<f32>,
    /// Called when value changes during drag
    on_change: Callback<f32>,
    /// Called when drag gesture starts (activation delay passed)
    on_drag_start: Callback<()>,
    /// Called when drag gesture ends (touch/mouse up)
    on_drag_end: Callback<()>,
    /// Additional CSS class for styling variants (e.g., "eq-slider-gain")
    #[prop(default = "")]
    css_class: &'static str,
    /// Default value for double-tap reset (None = no double-tap)
    #[prop(optional)]
    default_value: Option<f32>,
) -> impl IntoView {
    let (is_activated, set_is_activated) = signal(false);
    let (is_pending, set_is_pending) = signal(false);

    // Internal local value signal — source of truth for display during drag
    let local_value = RwSignal::new(value.get_untracked());

    // No sync Effect — EqSlider initializes from value.get_untracked() above.
    // Parent signal changes don't sync during the modal's lifetime because
    // any signal.set() would trigger reactive_graph recursion detection.
    // On modal close+reopen, the component recreates with fresh values.

    let timeout_handle: Rc<RefCell<Option<gloo_timers::callback::Timeout>>> =
        Rc::new(RefCell::new(None));
    let move_base_x: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
    let touch_start_x: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
    let touch_start_y: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
    let drag_value: Rc<Cell<f32>> = Rc::new(Cell::new(value.get_untracked()));
    let last_touch_time: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));

    // Double-tap detection state
    let last_tap_time: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));

    let track_ref = NodeRef::<leptos::html::Div>::new();

    // Store document-level closures for mouse (same pattern as fader.rs)
    let mouse_move_closure: Rc<RefCell<Option<Closure<dyn FnMut(web_sys::MouseEvent)>>>> =
        Rc::new(RefCell::new(None));
    let mouse_up_closure: Rc<RefCell<Option<Closure<dyn FnMut(web_sys::MouseEvent)>>>> =
        Rc::new(RefCell::new(None));

    // --- Rc clones for closures ---
    let timeout_ts = timeout_handle.clone();
    let timeout_tm = timeout_handle.clone();
    let timeout_te = timeout_handle.clone();
    let timeout_md = timeout_handle;

    let base_x_ts = move_base_x.clone();
    let base_x_tm = move_base_x.clone();
    let base_x_te = move_base_x.clone();
    let base_x_md = move_base_x;

    let start_x_ts = touch_start_x.clone();
    let start_x_tm = touch_start_x;
    let start_y_ts = touch_start_y.clone();
    let start_y_tm = touch_start_y;

    let drag_ts = drag_value.clone();
    let drag_tm = drag_value.clone();
    let drag_md = drag_value;

    let last_touch_ts = last_touch_time.clone();
    let last_touch_te = last_touch_time.clone();
    let last_touch_md = last_touch_time;

    let last_tap_ts = last_tap_time.clone();

    let mm_closure_md = mouse_move_closure.clone();
    let mu_closure_md = mouse_up_closure.clone();

    // --- Touch handlers ---
    let handle_touchstart = move |ev: web_sys::TouchEvent| {
        let now = js_sys::Date::now();
        *last_touch_ts.borrow_mut() = now;

        // Double-tap detection: if within DOUBLE_TAP_MS and default_value is set
        if let Some(def) = default_value {
            let prev_time = last_tap_ts.get();
            if now - prev_time < DOUBLE_TAP_MS && prev_time > 0.0 && !is_activated.get_untracked()
            {
                // Double-tap detected — reset to default
                ev.prevent_default();
                last_tap_ts.set(0.0);
                local_value.set(def);
                on_change.run(def);
                return;
            }
            last_tap_ts.set(now);
        }

        if let Some(touch) = ev.touches().get(0) {
            let x = touch.client_x() as f64;
            *start_x_ts.borrow_mut() = Some(x);
            *start_y_ts.borrow_mut() = Some(touch.client_y() as f64);
            *base_x_ts.borrow_mut() = Some(x);
        }

        drag_ts.set(local_value.get_untracked());
        set_is_pending.set(true);

        let timeout = gloo_timers::callback::Timeout::new(ACTIVATION_DELAY_MS, move || {
            set_is_activated.set(true);
            set_is_pending.set(false);
            on_drag_start.run(());
            // Haptic feedback
            if let Some(window) = web_sys::window() {
                let navigator = window.navigator();
                let _ = navigator.vibrate_with_duration(30);
            }
        });
        *timeout_ts.borrow_mut() = Some(timeout);
    };

    let handle_touchmove = move |ev: web_sys::TouchEvent| {
        if let Some(touch) = ev.touches().get(0) {
            let current_x = touch.client_x() as f64;
            let current_y = touch.client_y() as f64;

            // Check if movement is mostly vertical (scrolling intent)
            if let (Some(sx), Some(sy)) = (*start_x_tm.borrow(), *start_y_tm.borrow()) {
                let dx = (current_x - sx).abs();
                let dy = (current_y - sy).abs();
                if dy > dx + 10.0 && !is_activated.get() {
                    *timeout_tm.borrow_mut() = None;
                    set_is_pending.set(false);
                    return;
                }
            }

            if !is_activated.get() {
                *base_x_tm.borrow_mut() = Some(current_x);
                return;
            }

            ev.prevent_default();

            if let Some(el) = track_ref.get() {
                let base_opt = *base_x_tm.borrow();
                if let Some(base_x) = base_opt {
                    let rect = el.get_bounding_client_rect();
                    let delta_x = current_x - base_x;
                    let delta_ratio = delta_x / rect.width();
                    let raw = drag_tm.get();
                    let new_val = (raw + delta_ratio as f32).clamp(0.0, 1.0);
                    drag_tm.set(new_val);
                    let quantized = (new_val * 200.0).round() / 200.0; // 0.005 steps
                    local_value.set(quantized);
                    on_change.run(quantized);
                    *base_x_tm.borrow_mut() = Some(current_x);
                }
            }
        }
    };

    let handle_touchend = move |_ev: web_sys::TouchEvent| {
        *last_touch_te.borrow_mut() = js_sys::Date::now();
        *timeout_te.borrow_mut() = None;
        let was_active = is_activated.get_untracked();
        set_is_pending.set(false);
        set_is_activated.set(false);
        *base_x_te.borrow_mut() = None;
        if was_active {
            on_drag_end.run(());
        }
    };

    let handle_touchcancel = move |_ev: web_sys::TouchEvent| {
        let was_active = is_activated.get_untracked();
        set_is_pending.set(false);
        set_is_activated.set(false);
        if was_active {
            on_drag_end.run(());
        }
    };

    // --- Mouse handler ---
    let handle_mousedown = move |ev: web_sys::MouseEvent| {
        if ev.button() != 0 {
            return;
        }
        // Guard against synthesized mouse events from touch
        if js_sys::Date::now() - *last_touch_md.borrow() < 500.0 {
            return;
        }

        ev.prevent_default();
        ev.stop_propagation();

        let document = web_sys::window().unwrap().document().unwrap();
        let doc_target: web_sys::EventTarget = document.clone().into();

        // Clean up previous listeners
        if let Some(old_mc) = mm_closure_md.borrow_mut().take() {
            let _ = doc_target
                .remove_event_listener_with_callback("mousemove", old_mc.as_ref().unchecked_ref());
        }
        if let Some(old_uc) = mu_closure_md.borrow_mut().take() {
            let _ = doc_target
                .remove_event_listener_with_callback("mouseup", old_uc.as_ref().unchecked_ref());
        }

        drag_md.set(local_value.get_untracked());
        *base_x_md.borrow_mut() = Some(ev.client_x() as f64);
        set_is_pending.set(true);

        let timeout = gloo_timers::callback::Timeout::new(ACTIVATION_DELAY_MS, move || {
            set_is_activated.set(true);
            set_is_pending.set(false);
            on_drag_start.run(());
        });
        *timeout_md.borrow_mut() = Some(timeout);

        let base_x_mm = base_x_md.clone();
        let base_x_mu = base_x_md.clone();
        let drag_mm = drag_md.clone();
        let mm_cleanup = mm_closure_md.clone();
        let mu_cleanup = mu_closure_md.clone();
        let doc_cleanup = doc_target.clone();
        let timeout_mu = timeout_md.clone();

        let track_ref_move = track_ref;
        let mc = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
            let current_x = ev.client_x() as f64;

            if !is_activated.get() {
                *base_x_mm.borrow_mut() = Some(current_x);
                return;
            }

            if let Some(el) = track_ref_move.get() {
                let base_opt = *base_x_mm.borrow();
                if let Some(base_x) = base_opt {
                    let rect = el.get_bounding_client_rect();
                    let delta_x = current_x - base_x;
                    let delta_ratio = delta_x / rect.width();
                    let raw = drag_mm.get();
                    let new_val = (raw + delta_ratio as f32).clamp(0.0, 1.0);
                    drag_mm.set(new_val);
                    let quantized = (new_val * 200.0).round() / 200.0;
                    local_value.set(quantized);
                    on_change.run(quantized);
                    *base_x_mm.borrow_mut() = Some(current_x);
                }
            }
        }) as Box<dyn FnMut(web_sys::MouseEvent)>);

        let _ =
            doc_target.add_event_listener_with_callback("mousemove", mc.as_ref().unchecked_ref());
        *mm_closure_md.borrow_mut() = Some(mc);

        let uc = Closure::wrap(Box::new(move |_ev: web_sys::MouseEvent| {
            *timeout_mu.borrow_mut() = None;
            let was_active = is_activated.get();
            set_is_pending.set(false);
            set_is_activated.set(false);
            *base_x_mu.borrow_mut() = None;

            if let Some(mc) = mm_cleanup.borrow_mut().take() {
                let _ = doc_cleanup
                    .remove_event_listener_with_callback("mousemove", mc.as_ref().unchecked_ref());
            }
            mu_cleanup.borrow_mut().take();

            if was_active {
                on_drag_end.run(());
            }
        }) as Box<dyn FnMut(web_sys::MouseEvent)>);

        let _ = doc_target.add_event_listener_with_callback("mouseup", uc.as_ref().unchecked_ref());
        *mu_closure_md.borrow_mut() = Some(uc);
    };

    // Desktop double-click to reset to default
    let handle_dblclick = move |_ev: web_sys::MouseEvent| {
        if let Some(def) = default_value {
            local_value.set(def);
            on_change.run(def);
        }
    };

    let pct = move || (local_value.get() * 100.0).clamp(0.0, 100.0);

    view! {
        <div
            node_ref=track_ref
            class=move || {
                let mut classes = vec!["eq-slider-track"];
                if !css_class.is_empty() { classes.push(css_class); }
                if is_pending.get() { classes.push("activating"); }
                if is_activated.get() { classes.push("active"); }
                classes.join(" ")
            }
            on:touchstart=handle_touchstart
            on:touchmove=handle_touchmove
            on:touchend=handle_touchend
            on:touchcancel=handle_touchcancel
            on:mousedown=handle_mousedown
            on:dblclick=handle_dblclick
        >
            <div class="eq-slider-fill" style=move || format!("width:{}%", pct()) />
            <div class="eq-slider-thumb" style=move || format!("left:{}%", pct()) />
        </div>
    }
}
