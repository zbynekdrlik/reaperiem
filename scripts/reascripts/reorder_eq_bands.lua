-- Reorder EQ Bands
-- One-time migration: reorder ReaEQ bands on all tracks to put HPF first.
-- Preserves existing EQ curves by migrating parameter values to new positions.
--
-- NEW band order (matches professional EQ convention):
--   Band 0: High Pass    (BANDTYPE 3)
--   Band 1: Low Shelf    (BANDTYPE 1)
--   Band 2: Parametric   (BANDTYPE 0)
--   Band 3: Parametric   (BANDTYPE 0)
--   Band 4: High Shelf   (BANDTYPE 2)
--
-- OLD ReaEQ default order:
--   Band 0: Low Shelf    (BANDTYPE 1)
--   Band 1: Parametric   (BANDTYPE 0)
--   Band 2: Parametric   (BANDTYPE 0)
--   Band 3: High Shelf   (BANDTYPE 2)
--   Band 4: High Pass    (BANDTYPE 3)
--
-- Action ID: _RS_REAPERIEM_REORDER_EQ
-- Result written to EXTSTATE: reaperiem/eq_reorder_result

local section = "reaperiem"

-- BANDTYPE values for ReaEQ (BANDTYPE without colon)
-- 0=LowShelf, 1=HighShelf, 4=HighPass, 8=Band(peaking)
local TYPE_BAND = "8"
local TYPE_LOWSHELF = "0"
local TYPE_HIGHSHELF = "1"
local TYPE_HIGHPASS = "4"

-- Target band types (new order)
local TARGET_TYPES = { TYPE_HIGHPASS, TYPE_LOWSHELF, TYPE_BAND, TYPE_BAND, TYPE_HIGHSHELF }

-- Sensible defaults for each band position (new order)
-- freq_norm verified empirically against REAPER, gain_norm(0.25=0dB), bw_norm
local DEFAULTS = {
    { 0.1160, 0.25, 0.50 },  -- Band 0: HPF 80Hz, 0dB, 2.0 oct
    { 0.2316, 0.25, 0.50 },  -- Band 1: Low Shelf 200Hz, 0dB, 2.0 oct
    { 0.4408, 0.25, 0.25 },  -- Band 2: Band 800Hz, 0dB, 1.0 oct
    { 0.6548, 0.25, 0.25 },  -- Band 3: Band 3kHz, 0dB, 1.0 oct
    { 0.8176, 0.25, 0.50 },  -- Band 4: High Shelf 8kHz, 0dB, 2.0 oct
}

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

-- Read current band type for a band
local function get_band_type(track, eq_idx, band)
    local ok, val = reaper.TrackFX_GetNamedConfigParm(track, eq_idx, "BANDTYPE" .. band)
    if ok then return val end
    -- Fallback: detect from param name
    local _, name = reaper.TrackFX_GetParamName(track, eq_idx, band * 3)
    if name:match("Low Shelf") then return TYPE_LOWSHELF
    elseif name:match("High Shelf") then return TYPE_HIGHSHELF
    elseif name:match("High Pass") then return TYPE_HIGHPASS
    else return TYPE_BAND end
end

-- Check if a track has NON-DEFAULT EQ (custom tuned)
local function has_custom_eq(track, eq_idx, num_bands)
    for b = 0, num_bands - 1 do
        local gain_norm = reaper.TrackFX_GetParam(track, eq_idx, b * 3 + 1)
        -- If gain is not 0dB (norm != 0.25), it's custom
        if math.abs(gain_norm - 0.25) > 0.01 then
            return true
        end
        -- Check if band type was changed from default
        local btype = get_band_type(track, eq_idx, b)
        local default_types = { TYPE_LOWSHELF, TYPE_BAND, TYPE_BAND, TYPE_HIGHSHELF, TYPE_HIGHPASS }
        if b < 5 and btype ~= default_types[b + 1] then
            return true
        end
    end
    return false
end

-- Read all parameters for a band
local function read_band(track, eq_idx, band)
    local freq = reaper.TrackFX_GetParam(track, eq_idx, band * 3)
    local gain = reaper.TrackFX_GetParam(track, eq_idx, band * 3 + 1)
    local bw = reaper.TrackFX_GetParam(track, eq_idx, band * 3 + 2)
    local btype = get_band_type(track, eq_idx, band)
    local ok, en_val = reaper.TrackFX_GetNamedConfigParm(track, eq_idx, "BANDENABLED" .. band)
    local enabled = (not ok or en_val == "1")
    return { freq = freq, gain = gain, bw = bw, btype = btype, enabled = enabled }
end

-- Write all parameters for a band
local function write_band(track, eq_idx, band, data)
    reaper.TrackFX_SetNamedConfigParm(track, eq_idx, "BANDTYPE" .. band, data.btype)
    reaper.TrackFX_SetParam(track, eq_idx, band * 3, data.freq)
    reaper.TrackFX_SetParam(track, eq_idx, band * 3 + 1, data.gain)
    reaper.TrackFX_SetParam(track, eq_idx, band * 3 + 2, data.bw)
    local en = data.enabled and "1" or "0"
    reaper.TrackFX_SetNamedConfigParm(track, eq_idx, "BANDENABLED" .. band, en)
end

local function reorder_eq()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    local count = reaper.CountTracks(0)
    local reordered = 0
    local defaulted = 0
    local skipped = 0
    local details = {}

    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)
        local eq_idx = find_reaeq(track)

        if eq_idx >= 0 then
            local num_params = reaper.TrackFX_GetNumParams(track, eq_idx)
            local num_bands = math.floor(math.min(num_params, 15) / 3)

            if num_bands >= 5 then
                -- Check if already in target order
                local already_correct = true
                for b = 0, 4 do
                    if get_band_type(track, eq_idx, b) ~= TARGET_TYPES[b + 1] then
                        already_correct = false
                        break
                    end
                end

                if already_correct then
                    skipped = skipped + 1
                    table.insert(details, name .. ":already_correct")
                elseif has_custom_eq(track, eq_idx, num_bands) then
                    -- Custom EQ: read all bands, map by type to new positions
                    local old_bands = {}
                    for b = 0, 4 do
                        old_bands[b] = read_band(track, eq_idx, b)
                    end

                    -- Build migration map: old type -> new position
                    -- Collect bands by type
                    local lowshelves = {}
                    local bands = {}
                    local highshelves = {}
                    local highpasses = {}
                    local other = {}
                    for b = 0, 4 do
                        local bd = old_bands[b]
                        if bd.btype == TYPE_LOWSHELF then table.insert(lowshelves, bd)
                        elseif bd.btype == TYPE_HIGHSHELF then table.insert(highshelves, bd)
                        elseif bd.btype == TYPE_HIGHPASS then table.insert(highpasses, bd)
                        else table.insert(bands, bd) end
                    end

                    -- Assign to new positions:
                    -- Pos 0: HPF (or default if none exists)
                    -- Pos 1: LowShelf (or default if none)
                    -- Pos 2-3: Bands (fill from existing, default if short)
                    -- Pos 4: HighShelf (or default if none)
                    local new_bands = {}

                    -- Band 0: High Pass
                    if #highpasses > 0 then
                        new_bands[0] = highpasses[1]
                        new_bands[0].btype = TYPE_HIGHPASS
                    else
                        new_bands[0] = { freq = DEFAULTS[1][1], gain = DEFAULTS[1][2], bw = DEFAULTS[1][3],
                            btype = TYPE_HIGHPASS, enabled = false }
                    end

                    -- Band 1: Low Shelf
                    if #lowshelves > 0 then
                        new_bands[1] = lowshelves[1]
                        new_bands[1].btype = TYPE_LOWSHELF
                    else
                        new_bands[1] = { freq = DEFAULTS[2][1], gain = DEFAULTS[2][2], bw = DEFAULTS[2][3],
                            btype = TYPE_LOWSHELF, enabled = false }
                    end

                    -- Bands 2-3: Parametric
                    for p = 0, 1 do
                        if p < #bands then
                            new_bands[2 + p] = bands[p + 1]
                            new_bands[2 + p].btype = TYPE_BAND
                        else
                            new_bands[2 + p] = { freq = DEFAULTS[3 + p][1], gain = DEFAULTS[3 + p][2],
                                bw = DEFAULTS[3 + p][3], btype = TYPE_BAND, enabled = false }
                        end
                    end

                    -- Band 4: High Shelf
                    if #highshelves > 0 then
                        new_bands[4] = highshelves[1]
                        new_bands[4].btype = TYPE_HIGHSHELF
                    else
                        new_bands[4] = { freq = DEFAULTS[5][1], gain = DEFAULTS[5][2], bw = DEFAULTS[5][3],
                            btype = TYPE_HIGHSHELF, enabled = false }
                    end

                    -- Write all bands in new order
                    for b = 0, 4 do
                        write_band(track, eq_idx, b, new_bands[b])
                    end

                    reordered = reordered + 1
                    table.insert(details, name .. ":migrated")
                else
                    -- Default EQ: just set types and sensible defaults, all disabled
                    for b = 0, 4 do
                        reaper.TrackFX_SetNamedConfigParm(track, eq_idx, "BANDTYPE:" .. b, TARGET_TYPES[b + 1])
                        reaper.TrackFX_SetParam(track, eq_idx, b * 3, DEFAULTS[b + 1][1])
                        reaper.TrackFX_SetParam(track, eq_idx, b * 3 + 1, DEFAULTS[b + 1][2])
                        reaper.TrackFX_SetParam(track, eq_idx, b * 3 + 2, DEFAULTS[b + 1][3])
                        reaper.TrackFX_SetNamedConfigParm(track, eq_idx, "BANDENABLED" .. b, "0")
                    end
                    defaulted = defaulted + 1
                    table.insert(details, name .. ":defaulted")
                end
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Reorder EQ Bands", -1)

    local result = string.format("OK:reordered=%d,defaulted=%d,skipped=%d|%s",
        reordered, defaulted, skipped, table.concat(details, ";"))
    reaper.SetExtState(section, "eq_reorder_result", result, false)
end

local ok, err = pcall(reorder_eq)
if not ok then
    reaper.SetExtState(section, "eq_reorder_result", "ERROR:" .. tostring(err), false)
end

