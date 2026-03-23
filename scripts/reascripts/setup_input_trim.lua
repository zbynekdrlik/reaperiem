-- Setup Input Trim
-- Inserts JS:Utility/volume_pan as first FX on all mic/guitar input tracks.
-- This provides a pre-FX gain stage ("TRIM IN") that the engineer can adjust
-- in REAPER to normalize input levels before they reach member mixes.
--
-- Idempotent: skips tracks that already have TRIM IN at FX position 0.
-- Existing FX (ReaEQ, etc.) are pushed to position 1+ (not removed).
--
-- Action ID: _RS_REAPERIEM_SETUP_TRIM
-- Result written to EXTSTATE: reaperiem/trim_setup_result

local section = "reaperiem"

local function is_mic_or_gtr(name)
    local lower = name:lower()
    return lower:match("mic") or lower:match("gtr")
end

local function has_trim_at_pos0(track)
    local fx_count = reaper.TrackFX_GetCount(track)
    if fx_count == 0 then return false end
    local _, fx_name = reaper.TrackFX_GetFXName(track, 0)
    return fx_name:match("volume_pan") or fx_name:match("TRIM IN")
end

local function setup_trim()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    local count = reaper.CountTracks(0)
    local inserted = 0
    local skipped = 0
    local errors = {}
    local inserted_names = {}
    local skipped_names = {}

    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)

        if is_mic_or_gtr(name) then
            if has_trim_at_pos0(track) then
                skipped = skipped + 1
                table.insert(skipped_names, name)
            else
                -- Insert at position 0 (pushes existing FX down)
                -- -1000 - pos = insert at specific position
                local fx_idx = reaper.TrackFX_AddByName(track, "JS:Utility/volume_pan", false, -1000)
                if fx_idx >= 0 then
                    -- Rename to "TRIM IN" for clarity in FX chain
                    reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "renamed_name", "TRIM IN")
                    -- Set initial gain to 0 dB (param 0 = volume, 0.5 in normalized = 0dB for this plugin)
                    -- JS:volume_pan param 0 is volume in dB, default is 0
                    inserted = inserted + 1
                    table.insert(inserted_names, name)
                else
                    table.insert(errors, "Failed to insert on: " .. name)
                end
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Setup Input Trim", -1)

    -- Write result
    local result = string.format("OK:inserted=%d,skipped=%d", inserted, skipped)
    if #inserted_names > 0 then
        result = result .. ",inserted_tracks=" .. table.concat(inserted_names, ";")
    end
    if #skipped_names > 0 then
        result = result .. ",skipped_tracks=" .. table.concat(skipped_names, ";")
    end
    if #errors > 0 then
        result = result .. "|errors:" .. table.concat(errors, ";")
    end
    reaper.SetExtState(section, "trim_setup_result", result, false)
end

local ok, err = pcall(setup_trim)
if not ok then
    reaper.SetExtState(section, "trim_setup_result", "ERROR:" .. tostring(err), false)
end
