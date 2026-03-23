//! Full-screen parametric EQ modal with SVG frequency response curve
//!
//! Loads EQ state on-demand from REAPER via GetEqParams, displays draggable
//! band points on a log-frequency curve, and sends SetEqBand on slider changes.

use leptos::prelude::*;

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

/// Full-screen EQ modal component
#[component]
pub fn EQModal(
    /// Track index to show EQ for
    track_index: usize,
    /// Track name for the header
    track_name: String,
    /// EQ bands data (updated by parent from ServerMsg::EqParams)
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
                <Show when=move || !loading.get() && bands.get().is_empty() fallback=|| ()>
                    <div class="eq-no-eq">"No ReaEQ found on this track"</div>
                </Show>

                // SVG Curve display
                <Show when=move || !bands.get().is_empty() fallback=|| ()>
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

                            // Frequency response curve
                            <path
                                d=move || generate_curve_path(&bands.get(), svg_width, svg_height)
                                fill="none"
                                stroke="var(--accent)"
                                stroke-width="2.5"
                            />

                            // Filled area under curve
                            <path
                                d=move || {
                                    let curve = generate_curve_path(&bands.get(), svg_width, svg_height);
                                    format!("{} L{:.1},{:.1} L0,{:.1} Z", curve, svg_width, zero_db_y, zero_db_y)
                                }
                                fill="rgba(78, 205, 196, 0.08)"
                            />

                            // Band points
                            {move || bands.get().iter().enumerate().map(|(i, band)| {
                                let cx = freq_to_x(band.freq_hz, svg_width);
                                let cy = gain_to_y(band.gain_db, svg_height);
                                let color = band_color(&band.band_type);
                                view! {
                                    <circle
                                        cx=cx cy=cy r="8"
                                        fill=color
                                        stroke="white" stroke-width="2"
                                        opacity="0.9"
                                    />
                                    <text
                                        x=cx y=cy - 12.0
                                        fill="white" font-size="11" text-anchor="middle"
                                        font-weight="bold"
                                    >
                                        {format!("{}", i + 1)}
                                    </text>
                                }
                            }).collect::<Vec<_>>()}
                        </svg>
                    </div>

                    // Band controls
                    <div class="eq-band-controls">
                        {move || bands.get().iter().enumerate().map(|(i, band)| {
                            let band_idx = i as u8;
                            let color = band_color(&band.band_type).to_string();
                            let band_type = band.band_type.clone();
                            let freq_hz = band.freq_hz;
                            let gain_db = band.gain_db;
                            let bw = band.bw;
                            let freq_norm = band.freq_norm;
                            let gain_norm = band.gain_norm;
                            let bw_norm = band.bw_norm;

                            view! {
                                <div class="eq-band-card" style=format!("border-color: {}", color)>
                                    <div class="eq-band-header">
                                        <span class="eq-band-num" style=format!("background: {}", color)>
                                            {i + 1}
                                        </span>
                                        <span class="eq-band-type">{band_type.clone()}</span>
                                    </div>

                                    // Frequency slider
                                    <div class="eq-param-row">
                                        <label class="eq-param-label">"Freq"</label>
                                        <input
                                            type="range"
                                            class="eq-slider"
                                            min="0" max="1" step="0.001"
                                            value=freq_norm
                                            on:input=move |ev| {
                                                if let Ok(v) = event_target_value(&ev).parse::<f32>() {
                                                    on_param_change.run((band_idx, "freq".to_string(), v));
                                                }
                                            }
                                        />
                                        <span class="eq-param-value">{format_freq(freq_hz)}</span>
                                    </div>

                                    // Gain slider
                                    <div class="eq-param-row">
                                        <label class="eq-param-label">"Gain"</label>
                                        <input
                                            type="range"
                                            class="eq-slider eq-slider-gain"
                                            min="0" max="1" step="0.001"
                                            value=gain_norm
                                            on:input=move |ev| {
                                                if let Ok(v) = event_target_value(&ev).parse::<f32>() {
                                                    on_param_change.run((band_idx, "gain".to_string(), v));
                                                }
                                            }
                                        />
                                        <span class="eq-param-value">
                                            {if gain_db >= 0.0 { format!("+{:.1}", gain_db) } else { format!("{:.1}", gain_db) }}
                                            " dB"
                                        </span>
                                    </div>

                                    // Bandwidth/Q slider
                                    <div class="eq-param-row">
                                        <label class="eq-param-label">"BW"</label>
                                        <input
                                            type="range"
                                            class="eq-slider"
                                            min="0" max="1" step="0.001"
                                            value=bw_norm
                                            on:input=move |ev| {
                                                if let Ok(v) = event_target_value(&ev).parse::<f32>() {
                                                    on_param_change.run((band_idx, "bw".to_string(), v));
                                                }
                                            }
                                        />
                                        <span class="eq-param-value">{format!("{:.2}", bw)} " oct"</span>
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </Show>
            </div>
        </div>
    }
}
