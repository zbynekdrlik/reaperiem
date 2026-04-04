-- Setup Output Limiter
-- Inserts ReaLimit as the last FX on all output (inear) tracks.
-- Idempotent: skips tracks that already have ReaLimit.
--
-- Action ID: _RS_REAPERIEM_SETUP_LIMITER
-- Result written to EXTSTATE: reaperiem/limiter_setup_result

local section = "reaperiem"

local function needs_limiter(name)
    return name:lower():match("inear")
end

local function is_limiter(fx_name)
    return fx_name:match("ReaLimit") or fx_name:match("^LIMITER$") or fx_name:match("^LIMITER ")
end

local function has_limiter(track)
    local fx_count = reaper.TrackFX_GetCount(track)
    for i = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, i)
        if is_limiter(fx_name) then
            return true, i
        end
    end
    return false, -1
end

-- Find parameter index by name substring match
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

local function setup_limiter()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    local count = reaper.CountTracks(0)
    local inserted = 0
    local skipped = 0
    local inserted_names = {}
    local skipped_names = {}
    local errors = {}

    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)

        if needs_limiter(name) then
            local has, _ = has_limiter(track)
            if has then
                skipped = skipped + 1
                table.insert(skipped_names, name)
            else
                -- Insert ReaLimit as the LAST FX on the track
                local fx_count = reaper.TrackFX_GetCount(track)
                local fx_idx = reaper.TrackFX_AddByName(track, "ReaLimit", false, -1000 - fx_count)
                if fx_idx >= 0 then
                    -- Rename for consistent identification
                    reaper.TrackFX_SetNamedConfigParm(track, fx_idx, "renamed_name", "LIMITER")

                    -- Discover parameter indices by name
                    local thresh_idx = find_param_idx(track, fx_idx, "thresh")
                    local ceil_idx = find_param_idx(track, fx_idx, "ceil")
                    if ceil_idx < 0 then
                        ceil_idx = find_param_idx(track, fx_idx, "output")
                    end
                    if ceil_idx < 0 then
                        ceil_idx = find_param_idx(track, fx_idx, "limit")
                    end
                    local release_idx = find_param_idx(track, fx_idx, "release")

                    -- Set defaults using hardcoded normalized values
                    -- (discovered empirically from ReaLimit parameter mapping):
                    --   Threshold: norm = (dB + 60) / 72   → -12 dB = 0.6667
                    --   Ceiling:   norm = (dB + 24) / 24   → -6 dB  = 0.75
                    --   Release:   non-linear inverse       → 50 ms  = 0.006
                    if thresh_idx >= 0 then
                        reaper.TrackFX_SetParam(track, fx_idx, thresh_idx, 0.6667)
                    end
                    if ceil_idx >= 0 then
                        reaper.TrackFX_SetParam(track, fx_idx, ceil_idx, 0.75)
                    end
                    if release_idx >= 0 then
                        reaper.TrackFX_SetParam(track, fx_idx, release_idx, 0.006)
                    end

                    inserted = inserted + 1
                    table.insert(inserted_names, name)
                else
                    table.insert(errors, "Failed to insert ReaLimit on: " .. name)
                end
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Setup Output Limiter", -1)

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
    reaper.SetExtState(section, "limiter_setup_result", result, false)
end

local ok, err = pcall(setup_limiter)
if not ok then
    reaper.SetExtState(section, "limiter_setup_result", "ERROR:" .. tostring(err), false)
end
