-- Reset All EQ
-- Deletes existing ReaEQ and re-inserts fresh instances with correct band types
-- and all bands DISABLED. This ensures band types are correctly applied.
-- Special case: MIREC mic preserves existing curve as "MiTec EQ".
--
-- Action ID: _RS_REAPERIEM_RESET_ALL_EQ
-- Result written to EXTSTATE: reaperiem/eq_reset_result
--
-- THIS IS A ONE-SHOT SCRIPT — run once after deploy to clean up EQ state.

local section = "reaperiem"

local function is_reaeq(fx_name)
    return fx_name:match("ReaEQ") or fx_name:match("^EQ$") or fx_name:match("^EQ ")
end

local function find_reaeq(track)
    local fx_count = reaper.TrackFX_GetCount(track)
    for i = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, i)
        if is_reaeq(fx_name) then
            return i
        end
    end
    return -1
end

-- Band types for BANDTYPE (no colon): HPF=4, LowShelf=0, Band=8, HighShelf=1
local band_types = { "4", "0", "8", "8", "1" }

-- Default parameters: { freq_norm, gain_norm(0.25=0dB), bw_norm }
-- freq_norm verified empirically against REAPER's GetFormattedParamValue
local defaults = {
    { 0.1160, 0.25, 0.50 },  -- HPF 80Hz
    { 0.2316, 0.25, 0.50 },  -- Low Shelf 200Hz
    { 0.4408, 0.25, 0.25 },  -- Band 800Hz
    { 0.6548, 0.25, 0.25 },  -- Band 3kHz
    { 0.8176, 0.25, 0.50 },  -- High Shelf 8kHz
}

-- Insert a fresh ReaEQ at position with correct band types and defaults
local function insert_clean_eq(track, position)
    local fx_idx = reaper.TrackFX_AddByName(track, "ReaEQ", false, -1000 - position)
    if fx_idx < 0 then return -1 end

    -- Rename to "EQ"
    reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "renamed_name", "EQ")

    -- Set band types and parameters together per band
    -- Setting type + params in same loop ensures REAPER processes each band fully
    for b = 0, 4 do
        reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "BANDTYPE" .. b, band_types[b + 1])
        reaper.TrackFX_SetParam(track, fx_idx, b * 3, defaults[b + 1][1])
        reaper.TrackFX_SetParam(track, fx_idx, b * 3 + 1, defaults[b + 1][2])
        reaper.TrackFX_SetParam(track, fx_idx, b * 3 + 2, defaults[b + 1][3])
        -- DISABLE band — use BANDENABLED (NO colon) for per-band control
        reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "BANDENABLED" .. b, "0")
    end

    return fx_idx
end

local function reset_all_eq()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    local count = reaper.CountTracks(0)
    local reset_count = 0
    local mirec_handled = false
    local reset_names = {}
    local errors = {}

    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)
        local eq_idx = find_reaeq(track)

        if eq_idx < 0 then
            -- No ReaEQ on this track, skip

        elseif name:lower():match("mirec") and name:lower():match("mic") then
            -- MIREC mic: check if "MiTec EQ" already exists (idempotent)
            local has_mitec = false
            local fx_count = reaper.TrackFX_GetCount(track)
            for fi = 0, fx_count - 1 do
                local _, fx_name = reaper.TrackFX_GetFXName(track, fi)
                if fx_name:match("MiTec EQ") then
                    has_mitec = true
                    break
                end
            end

            if has_mitec then
                -- Already has MiTec EQ from previous run — just replace the "EQ" instance
                reaper.TrackFX_Delete(track, eq_idx)
                local new_idx = insert_clean_eq(track, eq_idx)
                if new_idx >= 0 then
                    mirec_handled = true
                    table.insert(reset_names, name .. " (replaced EQ, kept MiTec EQ)")
                else
                    table.insert(errors, "Failed to re-insert EQ on MIREC mic")
                end
            else
                -- First run: rename existing to MiTec EQ, insert new clean EQ before it
                reaper.TrackFX_SetNamedConfigParm(track, eq_idx, "renamed_name", "MiTec EQ")
                local new_idx = insert_clean_eq(track, eq_idx)
                if new_idx >= 0 then
                    mirec_handled = true
                    table.insert(reset_names, name .. " (new EQ + preserved MiTec EQ)")
                else
                    table.insert(errors, "Failed to insert new EQ on MIREC mic")
                end
            end

        else
            -- Normal track: delete existing EQ and re-insert fresh
            local position = eq_idx
            reaper.TrackFX_Delete(track, eq_idx)
            local new_idx = insert_clean_eq(track, position)
            if new_idx >= 0 then
                reset_count = reset_count + 1
                table.insert(reset_names, name)
            else
                table.insert(errors, "Failed to re-insert EQ on: " .. name)
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Reset All EQ to Defaults", -1)

    local result = string.format("OK:reset=%d,mirec=%s", reset_count, tostring(mirec_handled))
    if #reset_names > 0 then
        result = result .. ",tracks=" .. table.concat(reset_names, ";")
    end
    if #errors > 0 then
        result = result .. "|errors:" .. table.concat(errors, ";")
    end
    reaper.SetExtState(section, "eq_reset_result", result, false)
end

local ok, err = pcall(reset_all_eq)
if not ok then
    reaper.SetExtState(section, "eq_reset_result", "ERROR:" .. tostring(err), false)
end
