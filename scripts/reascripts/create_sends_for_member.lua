-- Create Sends for Member
-- Creates sends from all input tracks to a specified output track.
--
-- Triggered via HTTP API after setting EXTSTATE parameters:
--   REAPERIEM_SETUP/target_track  = target track name (e.g., "ENGINEER inear")
--   REAPERIEM_SETUP/input_count   = number of input tracks (e.g., "22")
--
-- Usage:
--   curl "http://iem.lan:8080/_/SET/EXTSTATE/REAPERIEM_SETUP/target_track/ENGINEER%20inear"
--   curl "http://iem.lan:8080/_/SET/EXTSTATE/REAPERIEM_SETUP/input_count/22"
--   curl "http://iem.lan:8080/_/_RS_REAPERIEM_CREATE_SENDS"

local function log(msg)
    reaper.ShowConsoleMsg("[create_sends] " .. msg .. "\n")
end

-- Same send creation pattern as setup_iem_project.lua
local function create_send(src, dst, send_vol)
    local send_idx = reaper.CreateTrackSend(src, dst)
    if send_idx < 0 then
        log("  ERROR: failed to create send")
        return -1
    end
    reaper.SetTrackSendInfo_Value(src, 0, send_idx, "D_VOL", send_vol or 1.0)
    reaper.SetTrackSendInfo_Value(src, 0, send_idx, "D_PAN", 0.0)
    -- Pre-fader post-FX mode (3) so each member's mix is independent
    reaper.SetTrackSendInfo_Value(src, 0, send_idx, "I_SENDMODE", 3)
    return send_idx
end

-- Check if a send already exists from src to dst
local function send_exists(src, dst)
    local num_sends = reaper.GetTrackNumSends(src, 0)
    for i = 0, num_sends - 1 do
        local dest = reaper.GetTrackSendInfo_Value(src, 0, i, "P_DESTTRACK")
        if dest == dst then
            return true
        end
    end
    return false
end

-- Find track by name (case-insensitive exact match)
local function find_track_by_name(name)
    local num_tracks = reaper.CountTracks(0)
    local name_lower = name:lower()
    for i = 0, num_tracks - 1 do
        local track = reaper.GetTrack(0, i)
        local _, track_name = reaper.GetTrackName(track)
        if track_name:lower() == name_lower then
            return track, i
        end
    end
    return nil, -1
end

-- State for batched processing
local send_queue = {}
local send_index = 1
local send_count = 0
local skip_count = 0
local target_name = ""

function process_send_batch()
    local BATCH_SIZE = 25
    local batch_end = math.min(send_index + BATCH_SIZE - 1, #send_queue)

    reaper.PreventUIRefresh(1)

    for i = send_index, batch_end do
        local pair = send_queue[i]
        if send_exists(pair.src, pair.dst) then
            skip_count = skip_count + 1
        else
            create_send(pair.src, pair.dst, 1.0)
            send_count = send_count + 1
        end
    end

    reaper.PreventUIRefresh(-1)

    send_index = batch_end + 1

    if send_index <= #send_queue then
        reaper.defer(process_send_batch)
    else
        -- Done
        reaper.Undo_EndBlock("Create sends for " .. target_name, -1)
        reaper.TrackList_AdjustWindows(false)
        reaper.UpdateArrange()

        local result = string.format("OK: %d sends created, %d skipped (already exist)", send_count, skip_count)
        log(result)
        log("Done.")
        reaper.SetExtState("REAPERIEM_SETUP", "result", result, false)
    end
end

local function main()
    -- Read parameters from EXTSTATE
    target_name = reaper.GetExtState("REAPERIEM_SETUP", "target_track")
    local input_count_str = reaper.GetExtState("REAPERIEM_SETUP", "input_count")

    if target_name == "" then
        log("ERROR: REAPERIEM_SETUP/target_track not set")
        reaper.SetExtState("REAPERIEM_SETUP", "result", "ERROR: target_track not set", false)
        return
    end

    local input_count = tonumber(input_count_str)
    if not input_count or input_count < 1 then
        log("ERROR: REAPERIEM_SETUP/input_count invalid: " .. tostring(input_count_str))
        reaper.SetExtState("REAPERIEM_SETUP", "result", "ERROR: invalid input_count", false)
        return
    end

    log("Target track: " .. target_name)
    log("Input count: " .. input_count)

    -- Find target track
    local dst_track, dst_idx = find_track_by_name(target_name)
    if not dst_track then
        log("ERROR: target track not found: " .. target_name)
        reaper.SetExtState("REAPERIEM_SETUP", "result", "ERROR: target track not found", false)
        return
    end
    log("Found target at track index " .. (dst_idx + 1))

    -- Build send queue: input tracks 1..input_count → target
    -- Track indices in REAPER are 0-based internally, but input tracks
    -- start at index 2 (after INPUTS folder at 0, MICS folder at 1)
    -- However, we don't hardcode indices — we iterate tracks 0..input_count
    -- and pick the first input_count non-folder, non-output tracks.
    --
    -- Simpler approach: iterate tracks 1 through input_count (1-based REAPER display)
    -- These are the tracks that have sends to band members already.
    -- We find them by checking: does this track have sends to category 0 (track sends)?
    local num_tracks = reaper.CountTracks(0)

    -- Collect input tracks: any track that has at least one send to another track
    -- and is NOT the target track itself, and is NOT a folder track
    local input_tracks = {}
    for i = 0, num_tracks - 1 do
        local track = reaper.GetTrack(0, i)
        if track ~= dst_track then
            local num_sends = reaper.GetTrackNumSends(track, 0) -- category 0 = track sends
            local folder_depth = reaper.GetMediaTrackInfo_Value(track, "I_FOLDERDEPTH")
            -- Input tracks have sends to band member inear tracks and are not folders
            if num_sends > 0 and folder_depth <= 0 then
                input_tracks[#input_tracks + 1] = track
            end
        end
        if #input_tracks >= input_count then
            break
        end
    end

    log("Found " .. #input_tracks .. " input tracks with existing sends")

    if #input_tracks == 0 then
        log("ERROR: no input tracks found with sends")
        reaper.SetExtState("REAPERIEM_SETUP", "result", "ERROR: no input tracks with sends", false)
        return
    end

    -- Build queue
    for _, src in ipairs(input_tracks) do
        send_queue[#send_queue + 1] = { src = src, dst = dst_track }
    end

    log("Queued " .. #send_queue .. " sends for creation...")

    reaper.Undo_BeginBlock()
    reaper.defer(process_send_batch)
end

-- Run
local ok, err = pcall(main)
if not ok then
    log("FATAL ERROR: " .. tostring(err))
    reaper.SetExtState("REAPERIEM_SETUP", "result", "ERROR: " .. tostring(err), false)
end
