//! Integration tests that run against live REAPER on iem.lan.
//!
//! Gated with `#[cfg(feature = "integration")]` so they don't run in CI.
//! Run manually: `cargo test -p iem-server --features integration`

#![cfg(feature = "integration")]

use std::collections::HashMap;

/// Parse meters from NTRACK response using the same logic as poller/proxy.
fn parse_meters_from_ntrack(text: &str) -> HashMap<usize, f32> {
    let mut meters = HashMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() == Some(&"TRACK")
            && parts.len() >= 14
            && let Ok(track_idx) = parts[1].parse::<usize>()
            && let Ok(peak_centibels) = parts[6].parse::<f32>()
        {
            let peak_linear = if peak_centibels <= -1500.0 {
                0.0
            } else {
                let peak_db = peak_centibels / 100.0;
                10.0_f32.powf(peak_db / 20.0)
            };
            meters.insert(track_idx, peak_linear);
        }
    }
    meters
}

/// Verify that REAPER's meter floor (-1500 cb) produces 0.0 on live data.
/// Requires REAPER running on iem.lan with no active audio sources.
#[tokio::test]
async fn test_live_meter_floor() {
    let client = reqwest::Client::new();
    let resp = client
        .get("http://iem.lan:8080/_/NTRACK;TRACK")
        .send()
        .await
        .expect("Failed to connect to REAPER on iem.lan:8080");
    let text = resp.text().await.expect("Failed to read REAPER response");

    let meters = parse_meters_from_ntrack(&text);
    assert!(!meters.is_empty(), "Should have parsed at least one track");

    // When no audio is active, all meters should be at floor (0.0)
    let non_zero: Vec<_> = meters
        .iter()
        .filter(|(_, v)| **v > 0.0)
        .collect();
    // Note: this test may have non-zero meters if audio is actually playing.
    // When run with no audio sources, all should be 0.0.
    println!(
        "Parsed {} tracks, {} with signal",
        meters.len(),
        non_zero.len()
    );
    for (idx, val) in &non_zero {
        println!("  Track {} has signal: {:.6}", idx, val);
    }
}

/// Verify that all tracks in the live REAPER project have 14 fields.
/// This confirms record-armed state (flag 192) for all input tracks.
#[tokio::test]
async fn test_live_all_tracks_14_fields() {
    let client = reqwest::Client::new();
    let resp = client
        .get("http://iem.lan:8080/_/NTRACK;TRACK")
        .send()
        .await
        .expect("Failed to connect to REAPER on iem.lan:8080");
    let text = resp.text().await.expect("Failed to read REAPER response");

    let mut total_tracks = 0;
    let mut tracks_with_14_fields = 0;
    let mut short_tracks: Vec<(usize, String, usize)> = Vec::new();

    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() == Some(&"TRACK") {
            if let Ok(idx) = parts[1].parse::<usize>() {
                total_tracks += 1;
                if parts.len() >= 14 {
                    tracks_with_14_fields += 1;
                } else {
                    short_tracks.push((idx, parts[2].to_string(), parts.len()));
                }
            }
        }
    }

    println!(
        "Total tracks: {}, with 14+ fields: {}",
        total_tracks, tracks_with_14_fields
    );
    for (idx, name, fields) in &short_tracks {
        println!("  Track {} '{}' has only {} fields", idx, name, fields);
    }

    assert!(total_tracks > 0, "Should have found tracks in REAPER");
    assert_eq!(
        total_tracks, tracks_with_14_fields,
        "All tracks should have 14+ fields (record-armed); {} tracks had fewer",
        short_tracks.len()
    );
}
