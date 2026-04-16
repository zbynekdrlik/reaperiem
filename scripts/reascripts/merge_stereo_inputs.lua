-- Merge L/R track pairs into stereo tracks
-- Pairs: DRUMS, BASS, INST, OTHER, BGVS, IEMONLY
-- Uses LIVE REAPER API - no restart needed after initial registration

local pairs_to_merge = {
    {l_pattern = " L$", r_pattern = " R$"},  -- Match tracks ending in " L" and " R"
}

-- Helper to find tracks by name pattern
local function find_track_pair(base_name)
    local l_track, r_track = nil, nil
    local l_idx, r_idx = -1, -1

    for i = 0, reaper.CountTracks(0) - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)

        if name == base_name .. " L" then
            l_track = track
            l_idx = i
        elseif name == base_name .. " R" then
            r_track = track
            r_idx = i
        end
    end

    return l_track, r_track, l_idx, r_idx
end

-- Get hardware input channel from track
local function get_input_channel(track)
    local input = reaper.GetMediaTrackInfo_Value(track, "I_RECINPUT")
    -- RECINPUT format: channel + mode*1024 (mode 0=mono, 1=stereo)
    local channel = math.floor(input % 1024)
    local mode = math.floor(input / 1024)
    return channel, mode
end

-- Set stereo input from consecutive channels
local function set_stereo_input(track, l_channel)
    -- Stereo mode: channel + 1024
    local stereo_input = l_channel + 1024
    reaper.SetMediaTrackInfo_Value(track, "I_RECINPUT", stereo_input)
end

-- Main merge operation
local function merge_stereo_pairs()
    local base_names = {"DRUMS", "BASS", "INST", "OTHER", "BGVS", "IEMONLY", "ALEX kl"}
    local merged_count = 0
    local tracks_to_delete = {}

    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    for _, base_name in ipairs(base_names) do
        local l_track, r_track, l_idx, r_idx = find_track_pair(base_name)

        if l_track and r_track then
            local l_chan, _ = get_input_channel(l_track)
            local r_chan, _ = get_input_channel(r_track)

            -- Rename L track to base name
            reaper.GetSetMediaTrackInfo_String(l_track, "P_NAME", base_name, true)

            -- Set stereo input (assumes consecutive channels)
            set_stereo_input(l_track, l_chan)

            -- Ensure track is stereo
            reaper.SetMediaTrackInfo_Value(l_track, "I_NCHAN", 2)

            -- Mark R track for deletion
            table.insert(tracks_to_delete, r_track)

            merged_count = merged_count + 1
            reaper.ShowConsoleMsg("Merged: " .. base_name .. " L + R -> " .. base_name ..
                " (inputs " .. l_chan .. "+" .. r_chan .. " -> stereo from " .. l_chan .. ")\n")
        else
            if not l_track and not r_track then
                -- Neither exists, check if already merged
                for i = 0, reaper.CountTracks(0) - 1 do
                    local track = reaper.GetTrack(0, i)
                    local _, name = reaper.GetTrackName(track)
                    if name == base_name then
                        reaper.ShowConsoleMsg("Already merged: " .. base_name .. "\n")
                        break
                    end
                end
            else
                reaper.ShowConsoleMsg("WARNING: Missing pair for " .. base_name ..
                    " (L=" .. tostring(l_track ~= nil) .. ", R=" .. tostring(r_track ~= nil) .. ")\n")
            end
        end
    end

    -- Delete R tracks (reverse order to maintain indices)
    for i = #tracks_to_delete, 1, -1 do
        reaper.DeleteTrack(tracks_to_delete[i])
    end

    reaper.PreventUIRefresh(-1)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Merge L/R stereo pairs", -1)

    reaper.ShowConsoleMsg("\nMerge complete! " .. merged_count .. " pairs merged.\n")
    reaper.ShowConsoleMsg("Track count: " .. reaper.CountTracks(0) .. "\n")

    -- Save project
    reaper.Main_SaveProject(0, false)
end

-- Execute
merge_stereo_pairs()
