-- ONE-SHOT migration script — DO NOT USE AS A TEMPLATE FOR OTHER INSTRUMENTS.
-- The Dante channel number and track name are HARDCODED. Each new instrument
-- needs its own ad-hoc setup script OR a generalized helper (not yet built).
--
-- One-shot setup for CG (stereo content-playback input, Dante RX 53/54).
-- Used to play YouTube videos, music, etc. on the LED wall during
-- presentations. Routed to all 10 member inears (9 band + engineer),
-- DEFAULT-MUTED — members unmute individually when content plays.
-- Idempotent: safe to re-run. Does nothing if CG already exists.
--
-- Creates:
--   1. A stereo REAPER track named "CG" at the end of the track list
--   2. Hardware input = Dante RX 53-54 stereo (channel 52 + 1024 for stereo)
--   3. Sends from CG to every <MEMBER> inear track found in the project, all MUTED
--   4. Saves the project
--
-- Does NOT insert TRIM IN or ReaEQ FX — caller must run
-- _RS_REAPERIEM_SETUP_TRIM and _RS_REAPERIEM_SETUP_EQ after this script.
-- Those scripts iterate all input tracks via is_input_track predicate
-- (PR #176) and will pick up CG automatically.
--
-- Pre-flight: aborts with ERROR if no <MEMBER> inear tracks are found.
--
-- Action ID: _RS_REAPERIEM_SETUP_CG
-- Result written to EXTSTATE: reaperiem/cg_setup_result

local section = "reaperiem"
local TRACK_NAME = "CG"
local DANTE_RX_L = 53  -- HARDCODED: 1-indexed Dante RX channel
local STEREO_INPUT = (DANTE_RX_L - 1) + 1024  -- = 52 + 1024 = 1076 (REAPER stereo input encoding)

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
    -- Include ENGINEER inear. Matches create_sends_for_member.lua convention.
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
    local inears_preflight = find_all_inear_tracks()
    if #inears_preflight == 0 then
        error("no '<MEMBER> inear' tracks found — refusing to create CG with no destinations")
    end

    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    -- Step 1: Ensure CG track exists
    local cg, cg_idx = find_track_by_name(TRACK_NAME)
    local track_created = false
    if not cg then
        local insert_at = reaper.CountTracks(0)
        reaper.InsertTrackAtIndex(insert_at, true)
        cg = reaper.GetTrack(0, insert_at)
        cg_idx = insert_at
        reaper.GetSetMediaTrackInfo_String(cg, "P_NAME", TRACK_NAME, true)
        track_created = true
    end

    -- Step 2: Set stereo channel count and hardware input
    reaper.SetMediaTrackInfo_Value(cg, "I_NCHAN", 2)
    reaper.SetMediaTrackInfo_Value(cg, "I_RECINPUT", STEREO_INPUT)
    reaper.SetMediaTrackInfo_Value(cg, "I_RECARM", 1)
    reaper.SetMediaTrackInfo_Value(cg, "I_RECMON", 1)

    -- Step 3: Create sends to every <MEMBER> inear track, ALL MUTED by default.
    local inears = inears_preflight
    local sends_created = 0
    local sends_skipped = 0
    for _, ie in ipairs(inears) do
        if has_send_to(cg, ie.track) then
            sends_skipped = sends_skipped + 1
        else
            local send_idx = reaper.CreateTrackSend(cg, ie.track)
            if send_idx >= 0 then
                -- Pre-fader post-FX (mode 3) — same as all input tracks.
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "I_SENDMODE", 3)
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "D_VOL", 1.0)
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "D_PAN", 0.0)
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "I_SRCCHAN", 0)
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "I_DSTCHAN", 0)
                -- CG DIFF vs setup_alex_kl.lua: ALL sends muted by default,
                -- not just engineer. Matches the design decision — content
                -- playback is opt-in per member, they unmute when content
                -- is actually playing (presentations, YouTube, etc.).
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "B_MUTE", 1)
                sends_created = sends_created + 1
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Setup CG", -1)

    reaper.Main_SaveProject(0, false)

    local result = string.format(
        "OK:track_created=%s,track_idx=%d,sends_created=%d,sends_skipped=%d,inears_found=%d",
        tostring(track_created), cg_idx + 1, sends_created, sends_skipped, #inears
    )
    reaper.SetExtState(section, "cg_setup_result", result, false)
end

local ok, err = pcall(setup)
if not ok then
    reaper.SetExtState(section, "cg_setup_result", "ERROR:" .. tostring(err), false)
end
