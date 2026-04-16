-- One-shot setup for ALEX kl (stereo keyboard input).
-- Idempotent: safe to re-run. Does nothing if ALEX kl already exists.
--
-- Creates:
--   1. A stereo REAPER track named "ALEX kl" at the end of the track list
--      (operator can reposition later if desired)
--   2. Hardware input = Dante RX 13-14 stereo (channel 12 + 1024 for stereo)
--   3. Sends from ALEX kl to every <MEMBER> inear track found in the project
--   4. Saves the project
--
-- Does NOT insert TRIM IN or ReaEQ FX — caller must run
-- _RS_REAPERIEM_SETUP_TRIM and _RS_REAPERIEM_SETUP_EQ after this script.
--
-- Action ID: _RS_REAPERIEM_SETUP_ALEX_KL
-- Result written to EXTSTATE: reaperiem/alex_kl_setup_result

local section = "reaperiem"
local TRACK_NAME = "ALEX kl"
local DANTE_RX_L = 13  -- 1-indexed Dante channel; REAPER input is (N-1) + 1024 for stereo
local STEREO_INPUT = (DANTE_RX_L - 1) + 1024  -- = 12 + 1024 = 1036

local function find_track_by_name(name)
    local count = reaper.CountTracks(0)
    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, n = reaper.GetTrackName(track)
        if n == name then return track, i end
    end
    return nil, -1
end

local function find_all_inear_tracks()
    local result = {}
    local count = reaper.CountTracks(0)
    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, n = reaper.GetTrackName(track)
        if n:lower():match("inear$") then
            table.insert(result, { track = track, name = n, idx = i })
        end
    end
    return result
end

local function has_send_to(src_track, dest_track)
    local send_count = reaper.GetTrackNumSends(src_track, 0)  -- 0 = sends
    for s = 0, send_count - 1 do
        local d = reaper.GetTrackSendInfo_Value(src_track, 0, s, "P_DESTTRACK")
        if d == dest_track then return true end
    end
    return false
end

local function setup()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    -- Step 1: Ensure ALEX kl track exists
    local alex_kl, alex_kl_idx = find_track_by_name(TRACK_NAME)
    local track_created = false
    if not alex_kl then
        -- Insert a new track at the end
        local insert_at = reaper.CountTracks(0)
        reaper.InsertTrackAtIndex(insert_at, true)
        alex_kl = reaper.GetTrack(0, insert_at)
        alex_kl_idx = insert_at
        reaper.GetSetMediaTrackInfo_String(alex_kl, "P_NAME", TRACK_NAME, true)
        track_created = true
    end

    -- Step 2: Set stereo channel count and hardware input
    reaper.SetMediaTrackInfo_Value(alex_kl, "I_NCHAN", 2)
    reaper.SetMediaTrackInfo_Value(alex_kl, "I_RECINPUT", STEREO_INPUT)
    -- Arm for recording so input levels are visible on meters
    reaper.SetMediaTrackInfo_Value(alex_kl, "I_RECARM", 1)
    -- Monitor off (input monitor not needed for send pipeline)
    reaper.SetMediaTrackInfo_Value(alex_kl, "I_RECMON", 0)

    -- Step 3: Create sends to every <MEMBER> inear track
    local inears = find_all_inear_tracks()
    local sends_created = 0
    local sends_skipped = 0
    for _, ie in ipairs(inears) do
        if has_send_to(alex_kl, ie.track) then
            sends_skipped = sends_skipped + 1
        else
            local send_idx = reaper.CreateTrackSend(alex_kl, ie.track)
            if send_idx >= 0 then
                -- Pre-FX (I_SENDMODE = 1 means pre-FX post-envelopes; 3 means pre-fader)
                -- Use pre-FX post-envelopes (1) to match existing sends convention.
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "I_SENDMODE", 1)
                -- Volume = unity (1.0)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "D_VOL", 1.0)
                -- Pan = center (0.0)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "D_PAN", 0.0)
                -- Source channel = stereo 1-2 (0 = stereo L/R in REAPER send chan spec)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "I_SRCCHAN", 0)
                -- Dest channel = stereo 1-2 (same convention)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "I_DSTCHAN", 0)
                sends_created = sends_created + 1
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Setup ALEX kl", -1)

    -- Save project
    reaper.Main_SaveProject(0, false)

    local result = string.format(
        "OK:track_created=%s,track_idx=%d,sends_created=%d,sends_skipped=%d,inears_found=%d",
        tostring(track_created), alex_kl_idx + 1, sends_created, sends_skipped, #inears
    )
    reaper.SetExtState(section, "alex_kl_setup_result", result, false)
end

local ok, err = pcall(setup)
if not ok then
    reaper.SetExtState(section, "alex_kl_setup_result", "ERROR:" .. tostring(err), false)
end
