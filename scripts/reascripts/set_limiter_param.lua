-- Set Limiter Parameter (Zero-Latency JS Limiter)
-- Sets a single JS:loser/MGA_JSLimiterST parameter on a track.
-- Parameters passed via EXTSTATE: reaperiem/limiter_set
--   Format: "track=N|param=P|value=V"
--   param: "threshold" (dB), "ceiling" (dB), "release" (ms), or "enabled" (0/1)
--   value: For threshold/ceiling/release, this is NORMALIZED 0-1 from the frontend.
--          The script converts to real values using known ranges.
--
-- JS limiter param indices:
--   p0: Threshold (dB)  range: -60 to 0
--   p1: Release (ms)    range: 1 to 500
--   p3: Ceiling (dB)    range: -24 to 0
--
-- Action ID: _RS_REAPERIEM_SET_LIMITER
-- Result written to EXTSTATE: reaperiem/limiter_set_result

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

local function set_limiter()
    local input = reaper.GetExtState(section, "limiter_set")
    if not input or input == "" then
        reaper.SetExtState(section, "limiter_set_result", "ERROR:no_input", false)
        return
    end

    local track_idx = tonumber(input:match("track=(%d+)"))
    local param_name = input:match("param=(%w+)")
    local value = tonumber(input:match("value=([%d%.%-]+)"))

    if not track_idx or not param_name or not value then
        reaper.SetExtState(section, "limiter_set_result", "ERROR:parse_failed:" .. input, false)
        return
    end

    local track = reaper.GetTrack(0, track_idx - 1)
    if not track then
        reaper.SetExtState(section, "limiter_set_result", "ERROR:track_not_found:" .. track_idx, false)
        return
    end

    local lim_idx = find_limiter(track)
    if lim_idx < 0 then
        reaper.SetExtState(section, "limiter_set_result", "ERROR:no_limiter:" .. track_idx, false)
        return
    end

    -- Handle "enabled" param via FX bypass toggle
    if param_name == "enabled" then
        reaper.TrackFX_SetEnabled(track, lim_idx, value >= 0.5)
        local state = reaper.TrackFX_GetEnabled(track, lim_idx) and "enabled" or "disabled"
        reaper.SetExtState(section, "limiter_set_result",
            string.format("OK:track=%d,param=enabled,value=%.6f,formatted=%s",
                track_idx, value, state), false)
        return
    end

    -- Only "limit" param is supported (sets both threshold AND ceiling to same value)
    -- This ensures volume = ceiling/threshold = 1.0 (unity gain, no boost)
    if param_name ~= "limit" then
        reaper.SetExtState(section, "limiter_set_result", "ERROR:unknown_param:" .. param_name .. ":use_limit", false)
        return
    end

    -- Convert normalized 0-1 → -24 to 0 dB (clamped to ceiling range -6 to 0)
    local limit_db = value * 24 - 24
    -- Clamp to valid range: ceiling is -6 to 0, threshold is -30 to 0
    limit_db = math.max(-6, math.min(0, limit_db))

    -- Set BOTH threshold and ceiling to the same value = unity gain
    reaper.TrackFX_SetParam(track, lim_idx, 0, limit_db)  -- Threshold (p0)
    reaper.TrackFX_SetParam(track, lim_idx, 3, limit_db)  -- Ceiling (p3)

    -- Read back for confirmation
    local actual_thresh = reaper.TrackFX_GetParam(track, lim_idx, 0)
    local actual_ceil = reaper.TrackFX_GetParam(track, lim_idx, 3)

    reaper.SetExtState(section, "limiter_set_result",
        string.format("OK:track=%d,param=limit,value=%.6f,threshold=%.2f,ceiling=%.2f",
            track_idx, value, actual_thresh, actual_ceil), false)
end

local ok, err = pcall(set_limiter)
if not ok then
    reaper.SetExtState(section, "limiter_set_result", "ERROR:" .. tostring(err), false)
end
