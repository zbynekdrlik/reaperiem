-- Check Input Trim
-- Reads current trim (JS:Volume/Pan at FX pos 0) from all mic/guitar tracks.
-- Reports which tracks have TRIM IN and their current gain values.
--
-- Action ID: _RS_REAPERIEM_CHECK_TRIM
-- Result written to EXTSTATE: reaperiem/trim_check
--   "OK:PETRONELA mic=0.0dB|MAREK mic=-3.0dB|..." — all tracks have trim
--   "FAIL:N:missing=STEVO mic;TINA mic" — N tracks missing trim FX

local section = "reaperiem"

-- NOTE: `is_input_track` is inlined (not `require`d) in every setup_input_*/
-- check_input_* script — REAPER's Lua `require` path doesn't reliably include
-- the reaperiem script folder, so duplication is the safer option.
local function is_input_track(track, name)
    -- Folder parent tracks (INPUTS, MICS, TECH, OUTPUTS, BAND) have
    -- I_FOLDERDEPTH > 0. Skip them — they're organisational buses.
    if reaper.GetMediaTrackInfo_Value(track, "I_FOLDERDEPTH") > 0 then
        return false
    end
    local lower = name:lower()
    if lower:match("inear$") or lower:match("stems$") then return false end
    if name == "MASTER" or name == "TRANSLATOR" then return false end
    if lower:match("^hand") or lower:match("^engineer") then return false end
    return true
end

local function check_trim()
    local count = reaper.CountTracks(0)
    local results = {}
    local missing = {}

    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)

        if is_input_track(track, name) then
            local fx_count = reaper.TrackFX_GetCount(track)
            local has_trim = false

            if fx_count > 0 then
                local _, fx_name = reaper.TrackFX_GetFXName(track, 0)
                if fx_name:match("volume_pan") or fx_name:match("TRIM IN") then
                    has_trim = true
                    -- Read gain parameter (param 0 = Volume slider in dB)
                    local gain_val = reaper.TrackFX_GetParam(track, 0, 0)
                    -- JS:volume_pan param 0: normalized 0-1 range
                    -- Convert: the plugin uses dB directly as slider value
                    -- Actually read the formatted value for accuracy
                    local _, buf = reaper.TrackFX_GetFormattedParamValue(track, 0, 0)
                    table.insert(results, name .. "=" .. buf)
                end
            end

            if not has_trim then
                table.insert(missing, name)
            end
        end
    end

    local result
    if #missing == 0 then
        result = "OK:" .. table.concat(results, "|")
    else
        result = string.format("FAIL:%d:missing=%s", #missing, table.concat(missing, ";"))
        if #results > 0 then
            result = result .. "|present=" .. table.concat(results, ";")
        end
    end

    reaper.SetExtState(section, "trim_check", result, false)
end

local ok, err = pcall(check_trim)
if not ok then
    reaper.SetExtState(section, "trim_check", "ERROR:" .. tostring(err), false)
end
