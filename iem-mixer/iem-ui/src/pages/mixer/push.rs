use wasm_bindgen::prelude::*;

/// Subscribe to Web Push for engineer SOS alerts (#133).
/// Fetches VAPID key, subscribes via Push API, sends subscription to server.
pub(super) fn subscribe_to_push() {
    wasm_bindgen_futures::spawn_local(async move {
        // 1. Fetch VAPID public key from server
        let token = match crate::auth::get_token() {
            Some(t) => t,
            None => {
                web_sys::console::log_1(&"[push] no auth token, skipping".into());
                return;
            }
        };

        let resp = match gloo_net::http::Request::get("/api/push/vapid-key")
            .send()
            .await
        {
            Ok(r) if r.ok() => r,
            Ok(r) => {
                web_sys::console::warn_1(
                    &format!("[push] vapid-key request failed: {}", r.status()).into(),
                );
                return;
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] vapid-key fetch error: {:?}", e).into());
                return;
            }
        };
        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] vapid-key parse error: {:?}", e).into());
                return;
            }
        };
        let vapid_key = match json.get("key").and_then(|k| k.as_str()) {
            Some(k) => k.to_string(),
            None => {
                web_sys::console::log_1(&"[push] VAPID not configured, skipping".into());
                return;
            }
        };
        web_sys::console::log_1(&format!("[push] got VAPID key: {}...", &vapid_key[..20]).into());

        // 2. Get ServiceWorkerRegistration
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let navigator = window.navigator();
        let sw_container: web_sys::ServiceWorkerContainer = match js_sys::Reflect::get(
            &navigator,
            &wasm_bindgen::JsValue::from_str("serviceWorker"),
        )
        .ok()
        .and_then(|v| v.dyn_into().ok())
        {
            Some(c) => c,
            None => {
                web_sys::console::log_1(&"[push] serviceWorker not available, skipping".into());
                return;
            }
        };

        let ready_promise = match sw_container.ready() {
            Ok(p) => p,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] sw.ready() failed: {:?}", e).into());
                return;
            }
        };
        web_sys::console::log_1(&"[push] waiting for SW ready...".into());
        let registration: web_sys::ServiceWorkerRegistration =
            match wasm_bindgen_futures::JsFuture::from(ready_promise).await {
                Ok(r) => match r.dyn_into() {
                    Ok(reg) => reg,
                    Err(e) => {
                        web_sys::console::warn_1(
                            &format!("[push] SW registration cast failed: {:?}", e).into(),
                        );
                        return;
                    }
                },
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("[push] SW ready await failed: {:?}", e).into(),
                    );
                    return;
                }
            };
        web_sys::console::log_1(&"[push] SW ready, getting push manager...".into());

        // 3. Subscribe to push
        let push_manager = match registration.push_manager() {
            Ok(pm) => pm,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] push_manager() failed: {:?}", e).into());
                return;
            }
        };

        // Unsubscribe any existing push subscription first (required when VAPID key changes,
        // otherwise Chrome rejects subscribe() with a different applicationServerKey)
        if let Ok(existing_promise) = push_manager.get_subscription() {
            if let Ok(existing_val) = wasm_bindgen_futures::JsFuture::from(existing_promise).await {
                if !existing_val.is_null() && !existing_val.is_undefined() {
                    if let Ok(existing_sub) = existing_val.dyn_into::<web_sys::PushSubscription>() {
                        let _ = wasm_bindgen_futures::JsFuture::from(
                            existing_sub.unsubscribe().unwrap_or_else(|_| {
                                js_sys::Promise::resolve(&wasm_bindgen::JsValue::TRUE)
                            }),
                        )
                        .await;
                        web_sys::console::log_1(
                            &"[push] unsubscribed old push subscription".into(),
                        );
                    }
                }
            }
        }

        // Decode base64url VAPID key to Uint8Array
        let key_bytes = match base64url_decode(&vapid_key) {
            Some(b) => b,
            None => return,
        };
        let key_array = js_sys::Uint8Array::new_with_length(key_bytes.len() as u32);
        key_array.copy_from(&key_bytes);

        let opts = web_sys::PushSubscriptionOptionsInit::new();
        opts.set_user_visible_only(true);
        opts.set_application_server_key(&key_array.into());

        let sub_promise = match push_manager.subscribe_with_options(&opts) {
            Ok(p) => p,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] subscribe failed: {:?}", e).into());
                return;
            }
        };
        web_sys::console::log_1(&"[push] subscribing to push...".into());
        let sub: web_sys::PushSubscription = match wasm_bindgen_futures::JsFuture::from(sub_promise)
            .await
        {
            Ok(v) => match v.dyn_into() {
                Ok(s) => s,
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("[push] subscription cast failed: {:?}", e).into(),
                    );
                    return;
                }
            },
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] subscribe await failed: {:?}", e).into());
                return;
            }
        };

        // 4. Send subscription JSON to server
        let sub_json = match sub.to_json() {
            Ok(j) => j,
            Err(_) => return,
        };
        let json_str = match js_sys::JSON::stringify(&sub_json)
            .ok()
            .and_then(|s| s.as_string())
        {
            Some(s) => s,
            None => return,
        };

        // Parse the JSON string to a serde_json::Value for gloo_net
        let body: serde_json::Value = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(_) => return,
        };

        let req = match gloo_net::http::Request::post("/api/push/subscribe")
            .header("Authorization", &format!("Bearer {}", token))
            .json(&body)
        {
            Ok(r) => r,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] serialize error: {:?}", e).into());
                return;
            }
        };
        match req.send().await {
            Ok(r) if r.ok() => {
                web_sys::console::log_1(&"[push] engineer subscribed to Web Push".into());
            }
            Ok(r) => {
                web_sys::console::warn_1(
                    &format!("[push] subscribe POST failed: {}", r.status()).into(),
                );
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] subscribe POST error: {:?}", e).into());
            }
        }
    });
}

/// Decode base64url (no padding) to bytes.
/// Note: atob() returns a Latin-1 string (each char = one byte 0-255).
/// Rust's `.bytes()` gives UTF-8 which mangles values > 127. Use `.chars() as u8` instead.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut s = input.replace('-', "+").replace('_', "/");
    while s.len() % 4 != 0 {
        s.push('=');
    }
    web_sys::window()?
        .atob(&s)
        .ok()
        .map(|decoded| decoded.chars().map(|c| c as u8).collect())
}
