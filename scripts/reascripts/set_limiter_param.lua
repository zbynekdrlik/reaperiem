-- Set Limiter Parameter
-- Sets a single ReaLimit parameter on a track.
-- Parameters passed via EXTSTATE: reaperiem/limiter_set
--   Format: "track=N|param=P|value=V"
--   param: "threshold", "ceiling", "release" (normalized 0-1), or "enabled" (0/1)
--
-- Action ID: _RS_REAPERIEM_SET_LIMITER
-- Result written to EXTSTATE: reaperiem/limiter_set_result

local section = "reaperiem"

local function find_limiter(track)
    local fx_count = reaper.TrackFX_GetCount(track)
    for i = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, i)
        if fx_name:match("ReaLimit") or fx_name:match("^LIMITER$") or fx_name:match("^LIMITER ") then
            return i
        end
    end
    return -1
end

local function find_param_idx(track, fx_idx, name_pattern)
    local num_params = reaper.TrackFX_GetNumParams(track, fx_idx)
    for p = 0, num_params - 1 do
        local _, pname = reaper.TrackFX_GetParamName(track, fx_idx, p)
        if pname:lower():match(name_pattern) then
            return p
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

    -- Map param name to search pattern
    local search_pattern
    if param_name == "threshold" then search_pattern = "thresh"
    elseif param_name == "ceiling" then search_pattern = "ceil"
    elseif param_name == "release" then search_pattern = "release"
    else
        if param_name == "output" then search_pattern = "output"
        elseif param_name == "limit" then search_pattern = "limit"
        else
            reaper.SetExtState(section, "limiter_set_result", "ERROR:unknown_param:" .. param_name, false)
            return
        end
    end

    local param_idx = find_param_idx(track, lim_idx, search_pattern)
    if param_idx < 0 and param_name == "ceiling" then
        param_idx = find_param_idx(track, lim_idx, "output")
    end
    if param_idx < 0 and param_name == "ceiling" then
        param_idx = find_param_idx(track, lim_idx, "limit")
    end

    if param_idx < 0 then
        reaper.SetExtState(section, "limiter_set_result", "ERROR:param_not_found:" .. param_name, false)
        return
    end

    -- Set the parameter (normalized 0-1)
    reaper.TrackFX_SetParam(track, lim_idx, param_idx, value)

    -- Read back formatted value for confirmation
    local _, fmt = reaper.TrackFX_GetFormattedParamValue(track, lim_idx, param_idx)

    reaper.SetExtState(section, "limiter_set_result",
        string.format("OK:track=%d,param=%s,value=%.6f,formatted=%s",
            track_idx, param_name, value, fmt), false)
end

local ok, err = pcall(set_limiter)
if not ok then
    reaper.SetExtState(section, "limiter_set_result", "ERROR:" .. tostring(err), false)
end
