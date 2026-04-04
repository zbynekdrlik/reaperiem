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

    -- Convert normalized slider value (0-1) to real parameter value
    -- and set the appropriate parameter index
    local param_idx
    local real_value
    if param_name == "threshold" then
        -- norm 0-1 → -60 to 0 dB
        param_idx = 0
        real_value = value * 60 - 60
    elseif param_name == "ceiling" then
        -- norm 0-1 → -24 to 0 dB
        param_idx = 3
        real_value = value * 24 - 24
    elseif param_name == "release" then
        -- norm 0-1 → 1 to 500 ms
        param_idx = 1
        real_value = value * 499 + 1
    else
        reaper.SetExtState(section, "limiter_set_result", "ERROR:unknown_param:" .. param_name, false)
        return
    end

    -- JS limiter takes direct values (dB, ms), not normalized
    reaper.TrackFX_SetParam(track, lim_idx, param_idx, real_value)

    -- Read back for confirmation
    local actual = reaper.TrackFX_GetParam(track, lim_idx, param_idx)

    reaper.SetExtState(section, "limiter_set_result",
        string.format("OK:track=%d,param=%s,value=%.6f,formatted=%.2f",
            track_idx, param_name, value, actual), false)
end

local ok, err = pcall(set_limiter)
if not ok then
    reaper.SetExtState(section, "limiter_set_result", "ERROR:" .. tostring(err), false)
end
