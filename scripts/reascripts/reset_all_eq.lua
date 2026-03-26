-- Reset All EQ
-- Resets all ReaEQ instances to clean defaults with all bands DISABLED.
-- Special case: MIREC mic gets a second EQ (preserves existing "MiTec EQ" curve).
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

-- Band types: HPF=3, LowShelf=1, Band=0, HighShelf=2
local band_types = { "3", "1", "0", "0", "2" }

-- Default parameters: { freq_norm, gain_norm(0.25=0dB), bw_norm }
local defaults = {
    { 0.12, 0.25, 0.50 },  -- HPF ~80Hz
    { 0.17, 0.25, 0.50 },  -- Low Shelf ~200Hz
    { 0.39, 0.25, 0.25 },  -- Band ~800Hz
    { 0.57, 0.25, 0.25 },  -- Band ~3kHz
    { 0.69, 0.25, 0.50 },  -- High Shelf ~8kHz
}

-- Apply clean defaults to a ReaEQ instance at fx_idx on track
local function apply_defaults(track, fx_idx)
    for b = 0, 4 do
        -- Set band type
        reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "BANDTYPE:" .. b, band_types[b + 1])
        -- Set parameters: freq, gain, bw
        reaper.TrackFX_SetParam(track, fx_idx, b * 3, defaults[b + 1][1])
        reaper.TrackFX_SetParam(track, fx_idx, b * 3 + 1, defaults[b + 1][2])
        reaper.TrackFX_SetParam(track, fx_idx, b * 3 + 2, defaults[b + 1][3])
        -- DISABLE band — use BANDENABLED (NO colon) for per-band control
        -- BANDENABLED:N (WITH colon) is a GLOBAL toggle — do NOT use
        reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "BANDENABLED" .. b, "0")
    end
    -- Rename to "EQ" if not already
    reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "renamed_name", "EQ")
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
            -- MIREC mic: preserve existing curve, add new clean EQ
            -- 1. Rename existing ReaEQ to "MiTec EQ" (invisible to find_reaeq)
            reaper.TrackFX_SetNamedConfigParm(track, eq_idx, "renamed_name", "MiTec EQ")

            -- 2. Insert new ReaEQ BEFORE the MiTec one (at same position)
            local new_idx = reaper.TrackFX_AddByName(track, "ReaEQ", false, -1000 - eq_idx)
            if new_idx >= 0 then
                -- 3. Apply clean defaults to new instance
                apply_defaults(track, new_idx)
                mirec_handled = true
                table.insert(reset_names, name .. " (new EQ + preserved MiTec EQ)")
            else
                table.insert(errors, "Failed to insert new ReaEQ on MIREC mic")
            end
        else
            -- Normal track: reset existing EQ to defaults
            apply_defaults(track, eq_idx)
            reset_count = reset_count + 1
            table.insert(reset_names, name)
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
