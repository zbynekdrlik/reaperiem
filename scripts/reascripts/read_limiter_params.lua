-- Read Limiter Parameters (Zero-Latency JS Limiter)
-- Reads JS:loser/MGA_JSLimiterST parameters from a specified track.
-- Track index passed via EXTSTATE: reaperiem/limiter_read_track
--
-- JS limiter params (direct values, not normalized):
--   p0: Threshold (dB)
--   p1: Release (ms)
--   p2: Link Stereo (%) — not exposed to UI
--   p3: Ceiling (dB)
--
-- Action ID: _RS_REAPERIEM_READ_LIMITER
-- Result written to EXTSTATE: reaperiem/limiter_params

local section = "reaperiem"

local function find_limiter(track)
    local fx_count = reaper.TrackFX_GetCount(track)
    for i = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, i)
        if fx_name:match("MGA_JSLimiter") or fx_name:match("^LIMITER$") or fx_name:match("^LIMITER ") then
            return i
        end
    end
    return -1
end

local function read_limiter()
    local track_idx_str = reaper.GetExtState(section, "limiter_read_track")
    local track_idx = tonumber(track_idx_str)
    if not track_idx then
        reaper.SetExtState(section, "limiter_params", "ERROR:no_track_index", false)
        return
    end

    local track = reaper.GetTrack(0, track_idx - 1) -- 1-based to 0-based
    if not track then
        reaper.SetExtState(section, "limiter_params", "ERROR:track_not_found:" .. track_idx, false)
        return
    end

    local _, tname = reaper.GetTrackName(track)
    local lim_idx = find_limiter(track)
    if lim_idx < 0 then
        reaper.SetExtState(section, "limiter_params", "NO_LIMITER:" .. tname, false)
        return
    end

    -- JS limiter params are direct values (dB, ms), not normalized 0-1
    -- p0=Threshold(dB), p1=Release(ms), p3=Ceiling(dB)
    local threshold = reaper.TrackFX_GetParam(track, lim_idx, 0)
    local release = reaper.TrackFX_GetParam(track, lim_idx, 1)
    local ceiling = reaper.TrackFX_GetParam(track, lim_idx, 3)

    -- For the frontend slider positioning, compute normalized values
    -- Threshold range: -60 to 0 dB → norm = (dB + 60) / 60
    -- Ceiling range: -24 to 0 dB → norm = (dB + 24) / 24
    -- Release range: 1 to 500 ms → norm = (ms - 1) / 499
    local thresh_norm = math.max(0, math.min(1, (threshold + 60) / 60))
    local ceil_norm = math.max(0, math.min(1, (ceiling + 24) / 24))
    local release_norm = math.max(0, math.min(1, (release - 1) / 499))

    -- Check FX enabled state (bypass)
    local enabled = reaper.TrackFX_GetEnabled(track, lim_idx) and "1" or "0"

    local result = string.format(
        "OK:track=%d,name=%s,fx=%d|threshold=%.2f,threshold_n=%.6f,ceiling=%.2f,ceiling_n=%.6f,release=%.1f,release_n=%.6f,enabled=%s",
        track_idx, tname, lim_idx,
        threshold, thresh_norm,
        ceiling, ceil_norm,
        release, release_norm,
        enabled
    )

    reaper.SetExtState(section, "limiter_params", result, false)
end

local ok, err = pcall(read_limiter)
if not ok then
    reaper.SetExtState(section, "limiter_params", "ERROR:" .. tostring(err), false)
end
