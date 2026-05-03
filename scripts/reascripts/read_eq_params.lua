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

        -- Get normalized values (0-1 range, parsed by server)
        local freq_norm = reaper.TrackFX_GetParam(track, eq_idx, freq_idx)
        local gain_norm = reaper.TrackFX_GetParam(track, eq_idx, gain_idx)
        local bw_norm = reaper.TrackFX_GetParam(track, eq_idx, bw_idx)

        -- Get REAPER-formatted display values (accurate, non-linear mapping)
        local _, freq_fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, freq_idx)
        local freq_num = freq_fmt:match("([%d%.%-]+)") or "0"
        local _, gain_fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, gain_idx)
        local gain_num = gain_fmt:match("([%d%.%-]+)") or "0"
        local _, bw_fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, bw_idx)
        local bw_num = bw_fmt:match("([%d%.%-]+)") or "0"

        -- Determine type from parameter name (reliable — names are fixed per band position)
        local btype = "band"
        if freq_name:match("Low Shelf") then btype = "lowshelf"
        elseif freq_name:match("High Shelf") then btype = "highshelf"
        elseif freq_name:match("High Pass") then btype = "highpass"
        elseif freq_name:match("Low Pass") then btype = "lowpass"
        elseif freq_name:match("Notch") then btype = "notch"
        elseif freq_name:match("Band Pass") then btype = "bandpass"
        end

        -- Read per-band enabled state via BANDENABLEDM (NO colon, M=band number).
        -- IMPORTANT: BANDENABLED:N (WITH colon) is a GLOBAL toggle — do NOT use.
        -- BANDENABLEDM (without colon) is the actual per-band enabled checkbox.
        local en_ok, en_val = reaper.TrackFX_GetNamedConfigParm(track, eq_idx, "BANDENABLED" .. b)
        local band_enabled = (not en_ok or en_val == "1") and "1" or "0"

        -- Sample REAPER's actual dB endpoints for this band's gain param.
        -- FormatParamValueNormalized returns the formatted display value WITHOUT
        -- mutating REAPER state — pure read.
        local _, gd_min_fmt = reaper.TrackFX_FormatParamValueNormalized(track, eq_idx, gain_idx, 0.0)
        local _, gd_max_fmt = reaper.TrackFX_FormatParamValueNormalized(track, eq_idx, gain_idx, 1.0)
        local gd_min_num = gd_min_fmt:match("(-?[%d%.]+)") or "-12"
        local gd_max_num = gd_max_fmt:match("(-?[%d%.]+)") or "12"

        table.insert(bands, string.format(
            "b%d:%s,fn=%.6f,gn=%.6f,bn=%.6f,fh=%s,gd=%s,bo=%s,en=%s,gd_min=%s,gd_max=%s",
            b, btype, freq_norm, gain_norm, bw_norm, freq_num, gain_num, bw_num, band_enabled, gd_min_num, gd_max_num
        ))
    end

    -- Get gain parameter range from first band
    local gain_range_str = ""
    if num_bands > 0 then
        local gain_idx_0 = 1 -- band 0, param offset 1 = gain
        local _, gmin, gmax, _ = reaper.TrackFX_GetParamEx(track, eq_idx, gain_idx_0)
        gain_range_str = string.format(",gmin=%.6f,gmax=%.6f", gmin, gmax)
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

    local result = string.format("OK:track=%d,name=%s,fx=%d,bands=%d,gg=%s,bypass=%s%s|%s",
        track_idx, tname, eq_idx, num_bands, global_gain_fmt, bypass_fmt, gain_range_str,
        table.concat(bands, "|"))

    reaper.SetExtState(section, "eq_params", result, false)
end

local ok, err = pcall(read_eq)
if not ok then
    reaper.SetExtState(section, "eq_params", "ERROR:" .. tostring(err), false)
end
