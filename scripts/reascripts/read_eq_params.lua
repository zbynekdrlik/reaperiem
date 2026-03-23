-- Read EQ Parameters
-- Reads all ReaEQ band parameters from a specified track.
-- Track index passed via EXTSTATE: reaperiem/eq_read_track
--
-- Action ID: _RS_REAPERIEM_READ_EQ
-- Result written to EXTSTATE: reaperiem/eq_params
--
-- ReaEQ layout (verified on live REAPER):
--   5 bands × 3 params (Freq, Gain, BW) = params 0-14
--   Band types: Low Shelf, Band, Band, High Shelf, High Pass
--   Global: p15=Global Gain, p16=Bypass, p17=Wet, p18=Delta

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

local function read_eq()
    local track_idx_str = reaper.GetExtState(section, "eq_read_track")
    local track_idx = tonumber(track_idx_str)
    if not track_idx then
        reaper.SetExtState(section, "eq_params", "ERROR:no_track_index", false)
        return
    end

    local track = reaper.GetTrack(0, track_idx - 1) -- Convert 1-based to 0-based
    if not track then
        reaper.SetExtState(section, "eq_params", "ERROR:track_not_found:" .. track_idx, false)
        return
    end

    local _, tname = reaper.GetTrackName(track)
    local eq_idx = find_reaeq(track)
    if eq_idx < 0 then
        reaper.SetExtState(section, "eq_params", "NO_EQ:" .. tname, false)
        return
    end

    local num_params = reaper.TrackFX_GetNumParams(track, eq_idx)

    -- Read bands (5 bands, 3 params each: freq, gain, bw)
    local bands = {}
    local num_bands = math.floor(math.min(num_params, 15) / 3)

    for b = 0, num_bands - 1 do
        local freq_idx = b * 3
        local gain_idx = b * 3 + 1
        local bw_idx = b * 3 + 2

        -- Get parameter names to determine band type
        local _, freq_name = reaper.TrackFX_GetParamName(track, eq_idx, freq_idx)
        local _, gain_name = reaper.TrackFX_GetParamName(track, eq_idx, gain_idx)

        -- Get formatted values (accurate dB/Hz)
        local _, freq_fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, freq_idx)
        local _, gain_fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, gain_idx)
        local _, bw_fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, bw_idx)

        -- Get normalized values
        local freq_norm = reaper.TrackFX_GetParam(track, eq_idx, freq_idx)
        local gain_norm = reaper.TrackFX_GetParam(track, eq_idx, gain_idx)
        local bw_norm = reaper.TrackFX_GetParam(track, eq_idx, bw_idx)

        -- Determine type from param name
        local btype = "band"
        if freq_name:match("Low Shelf") then btype = "lowshelf"
        elseif freq_name:match("High Shelf") then btype = "highshelf"
        elseif freq_name:match("High Pass") then btype = "highpass"
        elseif freq_name:match("Low Pass") then btype = "lowpass"
        elseif freq_name:match("Notch") then btype = "notch"
        elseif freq_name:match("Band Pass") then btype = "bandpass"
        end

        table.insert(bands, string.format(
            "b%d:%s,fn=%.6f,ff=%s,gn=%.6f,gf=%s,bn=%.6f,bf=%s",
            b, btype, freq_norm, freq_fmt, gain_norm, gain_fmt, bw_norm, bw_fmt
        ))
    end

    -- Read global params
    local global_gain_fmt = ""
    local bypass_fmt = ""
    if num_params > 15 then
        _, global_gain_fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, 15)
    end
    if num_params > 16 then
        _, bypass_fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, 16)
    end

    local result = string.format("OK:track=%d,name=%s,fx=%d,bands=%d,gg=%s,bypass=%s|%s",
        track_idx, tname, eq_idx, num_bands, global_gain_fmt, bypass_fmt,
        table.concat(bands, "|"))

    reaper.SetExtState(section, "eq_params", result, false)
end

local ok, err = pcall(read_eq)
if not ok then
    reaper.SetExtState(section, "eq_params", "ERROR:" .. tostring(err), false)
end
