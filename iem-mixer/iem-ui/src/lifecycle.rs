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

// ---------------------------------------------------------------------------
// Panic hook — side-effecting, not unit-testable.
// ---------------------------------------------------------------------------

use wasm_bindgen::JsCast;

/// Install a custom panic hook that:
/// 1. Logs the panic to the browser console (via `console_error_panic_hook`).
/// 2. Renders a hardcoded red overlay into `document.body` with a "Reload" button.
/// 3. POSTs a `ClientErrorReport` to `/api/client-error` (fire-and-forget).
///
/// Replaces `console_error_panic_hook::set_once()`. Must be called BEFORE
/// `leptos::mount::mount_to_body` so any panic during mount is captured.
pub fn install_panic_hook() {
    // Keep the nice console-side formatting from console_error_panic_hook.
    console_error_panic_hook::set_once();

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Delegate to the previous hook first so the console still gets a
        // structured backtrace. If that panics, we swallow it.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            prev(info);
        }));

        // Best-effort: render the overlay and POST diagnostics. Both are
        // wrapped in catch_unwind so a failure in one doesn't block the other
        // and nothing can re-panic inside the hook.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render_panic_overlay(info);
        }));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            post_panic_report(info);
        }));
    }));
}

fn render_panic_overlay(info: &std::panic::PanicHookInfo<'_>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(body) = document.body() else { return };

    let message = format_panic_message(info);
    let summary = truncate_for_display(&message, 200);
    let version = iem_core::version_label();

    // Raw HTML bypasses the (now broken) Leptos reactive graph.
    let overlay_html = format!(
        r#"<div id="iem-panic-overlay" style="
            position:fixed;inset:0;z-index:2147483647;
            background:rgba(20,0,0,0.95);color:#fff;
            font-family:system-ui,-apple-system,sans-serif;
            display:flex;flex-direction:column;
            align-items:center;justify-content:center;
            padding:24px;text-align:center;">
            <h1 style="color:#ff4444;margin:0 0 16px 0;font-size:22px;">
                IEM Mixer encountered an error
            </h1>
            <p style="opacity:0.85;max-width:480px;margin:0 0 24px 0;
                font-family:monospace;font-size:13px;word-break:break-word;">
                {}
            </p>
            <button id="iem-panic-reload" style="
                padding:14px 32px;font-size:16px;
                background:#ff4444;color:#fff;border:0;border-radius:8px;
                cursor:pointer;">
                Reload
            </button>
            <p style="opacity:0.5;font-size:11px;margin-top:16px;">{}</p>
        </div>"#,
        html_escape(&summary),
        html_escape(&version),
    );

    body.set_inner_html(&overlay_html);

    // Wire the reload button.
    if let Some(btn) = document.get_element_by_id("iem-panic-reload") {
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            if let Some(w) = web_sys::window() {
                let _ = w.location().reload();
            }
        }) as Box<dyn FnMut()>);
        if let Some(el) = btn.dyn_ref::<web_sys::HtmlElement>() {
            el.set_onclick(Some(closure.as_ref().unchecked_ref()));
        }
        // Leak the closure — the overlay is a terminal state, the page will reload.
        closure.forget();
    }
}

fn post_panic_report(info: &std::panic::PanicHookInfo<'_>) {
    let Some(window) = web_sys::window() else {
        return;
    };

    let url = window
        .location()
        .pathname()
        .unwrap_or_else(|_| String::from("/"));
    let user_agent = window.navigator().user_agent().unwrap_or_default();
    let message = format_panic_message(info);
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_default();

    let body = serde_json::json!({
        "panic_message": message,
        "version": iem_core::VERSION,
        "git_hash": iem_core::git_hash(),
        "url": url,
        "user_agent": user_agent,
        "location": location,
    });

    let body_string = match serde_json::to_string(&body) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Fire-and-forget fetch. We don't await; the page is about to reload anyway.
    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body_string));

    let Ok(headers) = web_sys::Headers::new() else {
        return;
    };
    let _ = headers.set("content-type", "application/json");
    opts.set_headers(&headers);

    if let Ok(request) = web_sys::Request::new_with_str_and_init("/api/client-error", &opts) {
        let _ = window.fetch_with_request(&request);
        // Intentionally drop the returned Promise without awaiting.
    }
}

fn format_panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        String::from("(unknown panic payload)")
    }
}

fn truncate_for_display(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

    #[test]
    fn truncate_for_display_below_limit_is_unchanged() {
        assert_eq!(truncate_for_display("short", 200), "short");
    }

    #[test]
    fn truncate_for_display_above_limit_is_truncated_with_ellipsis() {
        let long = "a".repeat(250);
        let result = truncate_for_display(&long, 200);
        assert_eq!(result.chars().count(), 201); // 200 + ellipsis
        assert!(result.ends_with('…'));
    }

    #[test]
    fn html_escape_replaces_all_special_chars() {
        assert_eq!(
            html_escape(r#"<script>alert("&")</script>"#),
            "&lt;script&gt;alert(&quot;&amp;&quot;)&lt;/script&gt;"
        );
    }
}
