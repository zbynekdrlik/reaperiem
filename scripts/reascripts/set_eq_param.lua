-- Set EQ Parameter
-- Sets a single ReaEQ band parameter on a track.
-- Parameters passed via EXTSTATE: reaperiem/eq_set
--   Format: "track=N|band=B|param=P|value=V"
--   param: "freq", "gain", "bw" (normalized 0-1 values)
--
-- Action ID: _RS_REAPERIEM_SET_EQ
-- Result written to EXTSTATE: reaperiem/eq_set_result

local section = "reaperiem"

local function find_reaeq(track)
    local fx_count = reaper.TrackFX_GetCount(track)
    for i = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, i)
        if fx_name:match("ReaEQ") or fx_name:match("^EQ$") or fx_name:match("^EQ ") then
            return i
        end
    end
    return -1
end

local function set_eq()
    local input = reaper.GetExtState(section, "eq_set")
    if not input or input == "" then
        reaper.SetExtState(section, "eq_set_result", "ERROR:no_input", false)
        return
    end

    -- Parse "track=N|band=B|param=P|value=V"
    local track_idx = tonumber(input:match("track=(%d+)"))
    local band = tonumber(input:match("band=(%d+)"))
    local param_name = input:match("param=(%w+)")
    local value = tonumber(input:match("value=([%d%.%-]+)"))

    if not track_idx or not band or not param_name or not value then
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:parse_failed:" .. input, false)
        return
    end

    local track = reaper.GetTrack(0, track_idx - 1)
    if not track then
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:track_not_found:" .. track_idx, false)
        return
    end

    local eq_idx = find_reaeq(track)
    if eq_idx < 0 then
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:no_reaeq:" .. track_idx, false)
        return
    end

    -- "enabled" param is handled by the UI via gain/freq changes (not BANDENABLED).
    -- BANDENABLED:N is a GLOBAL toggle that disables ALL bands — cannot use per-band.
    -- If "enabled" arrives here, it's a legacy message; translate to gain=0dB disable.
    if param_name == "enabled" then
        -- Determine band type from param name to choose disable method
        local freq_param_idx = band * 3
        local _, freq_name = reaper.TrackFX_GetParamName(track, eq_idx, freq_param_idx)
        local is_hpf = freq_name:match("High Pass")
        local is_lpf = freq_name:match("Low Pass")

        if value >= 0.5 then
            -- Enable: restore to a sensible default (UI should send specific values instead)
            if is_hpf then
                reaper.TrackFX_SetParam(track, eq_idx, freq_param_idx, 0.12) -- ~80Hz
            elseif is_lpf then
                reaper.TrackFX_SetParam(track, eq_idx, freq_param_idx, 0.95) -- ~18kHz
            end
            -- For shelf/band: no-op here (UI should send gain directly)
        else
            -- Disable: set to neutral
            if is_hpf then
                reaper.TrackFX_SetParam(track, eq_idx, freq_param_idx, 0.0) -- 20Hz = off
            elseif is_lpf then
                reaper.TrackFX_SetParam(track, eq_idx, freq_param_idx, 1.0) -- 20kHz = off
            else
                reaper.TrackFX_SetParam(track, eq_idx, band * 3 + 1, 0.25) -- gain 0dB = off
            end
        end

        local _, fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, freq_param_idx)
        reaper.SetExtState(section, "eq_set_result",
            string.format("OK:track=%d,band=%d,param=enabled,value=%.6f,formatted=%s",
                track_idx, band, value, value >= 0.5 and "enabled" or "disabled"), false)
        return
    end

    -- Calculate parameter index
    -- Each band has 3 params: freq (0), gain (1), bw (2)
    local param_offset = 0
    if param_name == "freq" then param_offset = 0
    elseif param_name == "gain" then param_offset = 1
    elseif param_name == "bw" then param_offset = 2
    else
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:unknown_param:" .. param_name, false)
        return
    end

    local param_idx = band * 3 + param_offset

    -- Validate param index
    local num_params = reaper.TrackFX_GetNumParams(track, eq_idx)
    if param_idx >= num_params then
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:param_out_of_range:" .. param_idx, false)
        return
    end

    -- Set the parameter (normalized 0-1)
    reaper.TrackFX_SetParam(track, eq_idx, param_idx, value)

    -- Read back formatted value for confirmation
    local _, fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, param_idx)

    reaper.SetExtState(section, "eq_set_result",
        string.format("OK:track=%d,band=%d,param=%s,value=%.6f,formatted=%s",
            track_idx, band, param_name, value, fmt), false)
end

local ok, err = pcall(set_eq)
if not ok then
    reaper.SetExtState(section, "eq_set_result", "ERROR:" .. tostring(err), false)
end
