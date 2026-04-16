//! Reactive state for the MixerPage component.
//!
//! Bundles all signal pairs into a single struct so that
//! `connect_websocket` can take one reference instead of 44 parameters.

use leptos::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::api::Channel;
use crate::components::category_tabs::Category;
use crate::components::eq_modal::EqBandState;
use crate::components::settings_modal::UserSettings;
use crate::components::talk_button::TalkState;

/// All reactive state owned by MixerPage.
#[derive(Clone, Copy)]
pub(super) struct MixerState {
    pub channels: (ReadSignal<Vec<Channel>>, WriteSignal<Vec<Channel>>),
    pub meters: (
        ReadSignal<HashMap<usize, [f32; 2]>>,
        WriteSignal<HashMap<usize, [f32; 2]>>,
    ),
    pub connected: (ReadSignal<bool>, WriteSignal<bool>),
    pub loading: (ReadSignal<bool>, WriteSignal<bool>),
    pub fader_touched: (
        ReadSignal<HashMap<usize, bool>>,
        WriteSignal<HashMap<usize, bool>>,
    ),
    pub global_touched: (ReadSignal<bool>, WriteSignal<bool>),
    pub stems_touched: (ReadSignal<bool>, WriteSignal<bool>),
    pub global_level: (ReadSignal<f32>, WriteSignal<f32>),
    pub global_muted: (ReadSignal<bool>, WriteSignal<bool>),
    pub stems_level: (ReadSignal<f32>, WriteSignal<f32>),
    pub stems_muted: (ReadSignal<bool>, WriteSignal<bool>),
    pub stems_bus_idx: (ReadSignal<Option<usize>>, WriteSignal<Option<usize>>),
    pub eq_open: (
        ReadSignal<Option<(usize, String)>>,
        WriteSignal<Option<(usize, String)>>,
    ),
    pub eq_bands: (ReadSignal<Vec<EqBandState>>, WriteSignal<Vec<EqBandState>>),
    pub eq_loading: (ReadSignal<bool>, WriteSignal<bool>),
    pub limiter_open: (
        ReadSignal<Option<(usize, String)>>,
        WriteSignal<Option<(usize, String)>>,
    ),
    pub limiter_limit_db: (ReadSignal<f32>, WriteSignal<f32>),
    pub limiter_limit_norm: (ReadSignal<f32>, WriteSignal<f32>),
    pub limiter_enabled: (ReadSignal<bool>, WriteSignal<bool>),
    pub limiter_loading: (ReadSignal<bool>, WriteSignal<bool>),
    pub limiter_active_seconds: (ReadSignal<f64>, WriteSignal<f64>),
    pub active_category: (ReadSignal<Category>, WriteSignal<Category>),
    pub data_pulse: (ReadSignal<bool>, WriteSignal<bool>),
    pub pinned_channels: (ReadSignal<Vec<usize>>, WriteSignal<Vec<usize>>),
    pub hidden_channels: (ReadSignal<Vec<usize>>, WriteSignal<Vec<usize>>),
    pub network_mode: (ReadSignal<String>, WriteSignal<String>),
    pub output_track_idx: (ReadSignal<Option<usize>>, WriteSignal<Option<usize>>),
    pub soloed: (ReadSignal<HashSet<usize>>, WriteSignal<HashSet<usize>>),
    pub pre_solo_mutes: (
        ReadSignal<HashMap<usize, bool>>,
        WriteSignal<HashMap<usize, bool>>,
    ),
    pub double_tap_fader: (ReadSignal<bool>, WriteSignal<bool>),
    pub has_photo: (ReadSignal<bool>, WriteSignal<bool>),
    pub preset_modal_visible: (ReadSignal<bool>, WriteSignal<bool>),
    pub pin_modal_visible: (ReadSignal<bool>, WriteSignal<bool>),
    pub settings_modal_visible: (ReadSignal<bool>, WriteSignal<bool>),
    pub snapshot_modal_visible: (ReadSignal<bool>, WriteSignal<bool>),
    pub alert_data: (
        ReadSignal<Option<(String, String)>>,
        WriteSignal<Option<(String, String)>>,
    ),
    pub alert_active: (ReadSignal<bool>, WriteSignal<bool>),
    pub talk_state: (ReadSignal<TalkState>, WriteSignal<TalkState>),
    pub engineer_talking: (ReadSignal<bool>, WriteSignal<bool>),
    pub ws: (
        ReadSignal<Option<web_sys::WebSocket>>,
        WriteSignal<Option<web_sys::WebSocket>>,
    ),
}

impl MixerState {
    pub fn new(member_id: &str) -> Self {
        let user_settings = UserSettings::load(member_id);
        Self {
            channels: signal(Vec::new()),
            meters: signal(HashMap::new()),
            connected: signal(false),
            loading: signal(true),
            fader_touched: signal(HashMap::new()),
            global_touched: signal(false),
            stems_touched: signal(false),
            global_level: signal(0.0),
            global_muted: signal(false),
            stems_level: signal(0.0),
            stems_muted: signal(false),
            stems_bus_idx: signal(None),
            eq_open: signal(None),
            eq_bands: signal(Vec::new()),
            eq_loading: signal(false),
            limiter_open: signal(None),
            limiter_limit_db: signal(-6.0),
            limiter_limit_norm: signal(0.0),
            limiter_enabled: signal(true),
            limiter_loading: signal(false),
            limiter_active_seconds: signal(0.0),
            active_category: signal(Category::Main),
            data_pulse: signal(false),
            pinned_channels: signal(Vec::new()),
            hidden_channels: signal(Vec::new()),
            network_mode: signal(String::new()),
            output_track_idx: signal(None),
            soloed: signal(HashSet::new()),
            pre_solo_mutes: signal(HashMap::new()),
            double_tap_fader: signal(user_settings.double_tap_fader),
            has_photo: signal(false),
            preset_modal_visible: signal(false),
            pin_modal_visible: signal(false),
            settings_modal_visible: signal(false),
            snapshot_modal_visible: signal(false),
            alert_data: signal(None),
            alert_active: signal(false),
            talk_state: signal(TalkState::Idle),
            engineer_talking: signal(false),
            ws: signal(None),
        }
    }
}
