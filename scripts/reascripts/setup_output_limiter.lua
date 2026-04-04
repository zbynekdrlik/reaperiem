-- Setup Output Limiter (Zero-Latency)
-- Inserts JS:loser/MGA_JSLimiterST on all output (inear) tracks.
-- Uses JS limiter instead of ReaLimit to avoid 960-sample lookahead latency.
-- Inserts BEFORE any VBAN sender (VBAN must be last in chain).
-- Removes any existing ReaLimit (migration from v1.131.0 initial deploy).
-- Idempotent: skips tracks that already have the JS limiter.
--
-- JS limiter params (direct values, not normalized):
--   p0: Threshold (dB)  — default -12.0
--   p1: Release (ms)    — default 50.0
--   p3: Ceiling (dB)    — default -0.1
--
-- Action ID: _RS_REAPERIEM_SETUP_LIMITER
-- Result written to EXTSTATE: reaperiem/limiter_setup_result

local section = "reaperiem"
local LIMITER_FX = "loser/MGA_JSLimiterST"

local function needs_limiter(name)
    return name:lower():match("inear")
end

local function is_js_limiter(fx_name)
    return fx_name:match("MGA_JSLimiter")
end

local function is_any_limiter(fx_name)
    return fx_name:match("ReaLimit") or fx_name:match("MGA_JSLimiter")
        or fx_name:match("^LIMITER$") or fx_name:match("^LIMITER ")
end

local function is_vban(fx_name)
    return fx_name:match("VBAN")
end

local function find_fx(track, check_fn)
    local fx_count = reaper.TrackFX_GetCount(track)
    for i = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, i)
        if check_fn(fx_name) then
            return i
        end
    end
    return -1
end

local function setup_limiter()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    local count = reaper.CountTracks(0)
    local inserted = 0
    local skipped = 0
    local migrated = 0
    local inserted_names = {}
    local skipped_names = {}
    local errors = {}

    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)

        if needs_limiter(name) then
            -- Step 1: Remove any old limiter (ReaLimit or renamed "LIMITER")
            local old_idx = find_fx(track, is_any_limiter)
            while old_idx >= 0 do
                reaper.TrackFX_Delete(track, old_idx)
                migrated = migrated + 1
                old_idx = find_fx(track, is_any_limiter)
            end

            -- Step 2: Check if JS limiter already exists (by exact plugin name)
            local existing = find_fx(track, is_js_limiter)
            if existing >= 0 then
                skipped = skipped + 1
                table.insert(skipped_names, name)
            else
                -- Step 3: Find insert position (before VBAN sender if present)
                local fx_count = reaper.TrackFX_GetCount(track)
                local vban_idx = find_fx(track, is_vban)
                local insert_pos
                if vban_idx >= 0 then
                    -- Insert before VBAN sender
                    insert_pos = vban_idx
                else
                    -- Insert at end
                    insert_pos = fx_count
                end

                local fx_idx = reaper.TrackFX_AddByName(track, LIMITER_FX, false, -1000 - insert_pos)
                if fx_idx >= 0 then
                    -- Rename for consistent identification
                    reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "renamed_name", "LIMITER")

                    -- Set defaults (JS limiter uses direct dB/ms values)
                    -- CRITICAL: threshold MUST equal ceiling for transparent limiting.
                    -- If threshold < ceiling, the limiter applies makeup gain (ceiling/threshold),
                    -- which BOOSTS the signal — unacceptable for a safety limiter.
                    -- p0: Threshold = -6.0 dB (safety limit level)
                    -- p1: Release = 50.0 ms
                    -- p3: Ceiling = -6.0 dB (must equal threshold for unity gain)
                    reaper.TrackFX_SetParam(track, fx_idx, 0, -6.0)   -- Threshold
                    reaper.TrackFX_SetParam(track, fx_idx, 1, 50.0)   -- Release
                    reaper.TrackFX_SetParam(track, fx_idx, 3, -6.0)   -- Ceiling = Threshold

                    inserted = inserted + 1
                    table.insert(inserted_names, name)
                else
                    table.insert(errors, "Failed to insert JS limiter on: " .. name)
                end
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Setup Output Limiter", -1)

    local result = string.format("OK:inserted=%d,skipped=%d,migrated=%d", inserted, skipped, migrated)
    if #inserted_names > 0 then
        result = result .. ",inserted_tracks=" .. table.concat(inserted_names, ";")
    end
    if #skipped_names > 0 then
        result = result .. ",skipped_tracks=" .. table.concat(skipped_names, ";")
    end
    if #errors > 0 then
        result = result .. "|errors:" .. table.concat(errors, ";")
    end
    reaper.SetExtState(section, "limiter_setup_result", result, false)
end

local ok, err = pcall(setup_limiter)
if not ok then
    reaper.SetExtState(section, "limiter_setup_result", "ERROR:" .. tostring(err), false)
end
