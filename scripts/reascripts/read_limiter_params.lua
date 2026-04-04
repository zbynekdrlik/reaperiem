-- Read Limiter Parameters
-- Reads ReaLimit parameters from a specified track.
-- Track index passed via EXTSTATE: reaperiem/limiter_read_track
--
-- Action ID: _RS_REAPERIEM_READ_LIMITER
-- Result written to EXTSTATE: reaperiem/limiter_params

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

    -- Discover parameter indices
    local thresh_idx = find_param_idx(track, lim_idx, "thresh")
    local ceil_idx = find_param_idx(track, lim_idx, "ceil")
    if ceil_idx < 0 then ceil_idx = find_param_idx(track, lim_idx, "output") end
    if ceil_idx < 0 then ceil_idx = find_param_idx(track, lim_idx, "limit") end
    local release_idx = find_param_idx(track, lim_idx, "release")

    -- Read values (normalized + formatted)
    local thresh_norm = thresh_idx >= 0 and reaper.TrackFX_GetParam(track, lim_idx, thresh_idx) or 0
    local ceil_norm = ceil_idx >= 0 and reaper.TrackFX_GetParam(track, lim_idx, ceil_idx) or 0
    local release_norm = release_idx >= 0 and reaper.TrackFX_GetParam(track, lim_idx, release_idx) or 0

    local thresh_fmt = "?"
    local ceil_fmt = "?"
    local release_fmt = "?"
    if thresh_idx >= 0 then _, thresh_fmt = reaper.TrackFX_GetFormattedParamValue(track, lim_idx, thresh_idx) end
    if ceil_idx >= 0 then _, ceil_fmt = reaper.TrackFX_GetFormattedParamValue(track, lim_idx, ceil_idx) end
    if release_idx >= 0 then _, release_fmt = reaper.TrackFX_GetFormattedParamValue(track, lim_idx, release_idx) end

    -- Extract numeric values from formatted strings
    local thresh_num = thresh_fmt:match("([%d%.%-]+)") or "0"
    local ceil_num = ceil_fmt:match("([%d%.%-]+)") or "0"
    local release_num = release_fmt:match("([%d%.%-]+)") or "0"

    -- Check FX enabled state (bypass)
    local enabled = reaper.TrackFX_GetEnabled(track, lim_idx) and "1" or "0"

    local result = string.format(
        "OK:track=%d,name=%s,fx=%d|threshold=%s,threshold_n=%.6f,threshold_i=%d,ceiling=%s,ceiling_n=%.6f,ceiling_i=%d,release=%s,release_n=%.6f,release_i=%d,enabled=%s",
        track_idx, tname, lim_idx,
        thresh_num, thresh_norm, thresh_idx,
        ceil_num, ceil_norm, ceil_idx,
        release_num, release_norm, release_idx,
        enabled
    )

    reaper.SetExtState(section, "limiter_params", result, false)
end

local ok, err = pcall(read_limiter)
if not ok then
    reaper.SetExtState(section, "limiter_params", "ERROR:" .. tostring(err), false)
end
