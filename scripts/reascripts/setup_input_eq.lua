-- Setup EQ
-- Inserts ReaEQ on mic/guitar input tracks and inear/stems output tracks.
-- Idempotent: skips tracks that already have ReaEQ.
--
-- Action ID: _RS_REAPERIEM_SETUP_EQ
-- Result written to EXTSTATE: reaperiem/eq_setup_result

local section = "reaperiem"

local function needs_eq(name)
    local lower = name:lower()
    return lower:match("mic") or lower:match("gtr")
        or lower:match("inear") or lower:match("stems")
end

local function is_reaeq(fx_name)
    return fx_name:match("ReaEQ") or fx_name:match("^EQ$") or fx_name:match("^EQ ")
end

local function has_reaeq(track)
    local fx_count = reaper.TrackFX_GetCount(track)
    for i = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, i)
        if is_reaeq(fx_name) then
            return true, i
        end
    end
    return false, -1
end

-- Remove duplicate ReaEQ instances (keep the first, remove later ones)
local function remove_duplicate_reaeq(track)
    local fx_count = reaper.TrackFX_GetCount(track)
    local found_first = false
    local removed = 0
    -- Iterate in reverse so removal indices stay valid
    for i = fx_count - 1, 0, -1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, i)
        if is_reaeq(fx_name) then
            if found_first then
                reaper.TrackFX_Delete(track, i)
                removed = removed + 1
            else
                found_first = true
            end
        end
    end
    return removed
end

local function setup_eq()
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

        if needs_eq(name) then
            -- Clean up any duplicate ReaEQ instances first
            remove_duplicate_reaeq(track)

            local has, eq_idx = has_reaeq(track)
            if has then
                skipped = skipped + 1
                table.insert(skipped_names, name)
            else
                -- Insert ReaEQ at position 1 (after TRIM IN at 0)
                -- If no TRIM IN exists, insert at position 0
                local fx_count = reaper.TrackFX_GetCount(track)
                local insert_pos = 0
                if fx_count > 0 then
                    local _, first_fx = reaper.TrackFX_GetFXName(track, 0)
                    if first_fx:match("volume_pan") or first_fx:match("TRIM IN") then
                        insert_pos = 1
                    end
                end

                local fx_idx = reaper.TrackFX_AddByName(track, "ReaEQ", false, -1000 - insert_pos)
                if fx_idx >= 0 then
                    -- Rename for clarity
                    reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "renamed_name", "EQ")

                    -- Set band types to professional order: HPF, LowShelf, Band, Band, HighShelf
                    local band_types = { "3", "1", "0", "0", "2" } -- HPF, LS, Band, Band, HS
                    -- Sensible defaults: freq_norm, gain_norm(0.25=0dB), bw_norm
                    local defaults = {
                        { 0.12, 0.25, 0.50 },  -- HPF ~80Hz
                        { 0.17, 0.25, 0.50 },  -- Low Shelf ~200Hz
                        { 0.39, 0.25, 0.25 },  -- Band ~800Hz
                        { 0.57, 0.25, 0.25 },  -- Band ~3kHz
                        { 0.69, 0.25, 0.50 },  -- High Shelf ~8kHz
                    }
                    for b = 0, 4 do
                        reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "BANDTYPE:" .. b, band_types[b + 1])
                        reaper.TrackFX_SetParam(track, fx_idx, b * 3, defaults[b + 1][1])
                        reaper.TrackFX_SetParam(track, fx_idx, b * 3 + 1, defaults[b + 1][2])
                        reaper.TrackFX_SetParam(track, fx_idx, b * 3 + 2, defaults[b + 1][3])
                        reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "BANDENABLED:" .. b, "0")
                    end

                    inserted = inserted + 1
                    table.insert(inserted_names, name)
                else
                    table.insert(errors, "Failed to insert ReaEQ on: " .. name)
                end
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Setup Input EQ", -1)

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
    reaper.SetExtState(section, "eq_setup_result", result, false)
end

local ok, err = pcall(setup_eq)
if not ok then
    reaper.SetExtState(section, "eq_setup_result", "ERROR:" .. tostring(err), false)
end
