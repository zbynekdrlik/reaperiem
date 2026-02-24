//! IEM Mixer WASM Frontend
//!
//! Leptos CSR application for controlling in-ear monitor mixes.

pub mod api;
pub mod auth;
pub mod components;
pub mod pages;
pub mod router;

use wasm_bindgen::prelude::*;

/// Main entry point for WASM
#[wasm_bindgen(start)]
pub fn main() {
    // Better panic messages in console
    console_error_panic_hook::set_once();

    // Mount the app
    leptos::mount::mount_to_body(router::App);
}
