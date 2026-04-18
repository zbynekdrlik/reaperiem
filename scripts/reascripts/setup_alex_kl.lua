-- ONE-SHOT migration script — DO NOT USE AS A TEMPLATE FOR OTHER INSTRUMENTS.
-- The Dante channel number and track name are HARDCODED. Each new instrument
-- needs its own ad-hoc setup script OR a generalized helper (not yet built).
--
-- One-shot setup for ALEX kl (stereo keyboard input).
-- Idempotent: safe to re-run. Does nothing if ALEX kl already exists.
--
-- Creates:
--   1. A stereo REAPER track named "ALEX kl" at the end of the track list
--   2. Hardware input = Dante RX 13-14 stereo (channel 12 + 1024 for stereo)
--   3. Sends from ALEX kl to every <MEMBER> inear track found in the project
--   4. Saves the project
--
-- Does NOT insert TRIM IN or ReaEQ FX — caller must run
-- _RS_REAPERIEM_SETUP_TRIM and _RS_REAPERIEM_SETUP_EQ after this script.
--
-- Pre-flight: aborts with ERROR if no <MEMBER> inear tracks are found
-- (indicates an empty/broken project — creating an isolated ALEX kl track
-- with no destinations would silently produce a useless setup).
--
-- Action ID: _RS_REAPERIEM_SETUP_ALEX_KL
-- Result written to EXTSTATE: reaperiem/alex_kl_setup_result

local section = "reaperiem"
local TRACK_NAME = "ALEX kl"
local DANTE_RX_L = 13  -- HARDCODED: 1-indexed Dante RX channel
local STEREO_INPUT = (DANTE_RX_L - 1) + 1024  -- = 12 + 1024 = 1036 (REAPER stereo input encoding)

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
    -- Include ENGINEER inear. Match the existing convention in
    -- create_sends_for_member.lua: every member-inear-destined send
    -- exists on all input tracks, but sends to ENGINEER are muted by
    -- default (engineer unmutes selectively on their own cue).
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
    -- Pre-flight: abort if the project has no <MEMBER> inear tracks.
    -- Creating ALEX kl with zero send destinations is useless and almost
    -- always indicates the script ran against an empty or broken project.
    local inears_preflight = find_all_inear_tracks()
    if #inears_preflight == 0 then
        error("no '<MEMBER> inear' tracks found — refusing to create ALEX kl with no destinations")
    end

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
    -- Monitor on: when transport is idle (the live IEM scenario), REAPER
    -- only routes the hardware input through the track's signal chain
    -- with I_RECMON=1. RECMON=0 would leave members hearing silence from
    -- ALEX kl during services. Matches set_hw_input_mono in setup_iem_project.lua.
    reaper.SetMediaTrackInfo_Value(alex_kl, "I_RECMON", 1)

    -- Step 3: Create sends to every <MEMBER> inear track.
    -- Reuse the preflight list — track pointers remain valid after inserting
    -- ALEX kl at the end (indices would shift but MediaTrack refs don't).
    local inears = inears_preflight
    local sends_created = 0
    local sends_skipped = 0
    for _, ie in ipairs(inears) do
        if has_send_to(alex_kl, ie.track) then
            sends_skipped = sends_skipped + 1
        else
            local send_idx = reaper.CreateTrackSend(alex_kl, ie.track)
            if send_idx >= 0 then
                -- Pre-fader post-FX (mode 3). TRIM IN + ReaEQ are inserted
                -- on ALEX kl by setup_input_trim / setup_input_eq; mode 3
                -- taps the signal AFTER those FX but BEFORE the fader,
                -- matching check_send_modes.lua's invariant for input
                -- tracks. Mode 1 (pre-FX) would bypass trim/EQ entirely
                -- AND trigger FAIL:N from check_send_modes.
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "I_SENDMODE", 3)
                -- Volume = unity (1.0)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "D_VOL", 1.0)
                -- Pan = center (0.0)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "D_PAN", 0.0)
                -- Source channel = stereo 1-2 (0 = stereo L/R in REAPER send chan spec)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "I_SRCCHAN", 0)
                -- Dest channel = stereo 1-2 (same convention)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "I_DSTCHAN", 0)
                -- Mute sends to ENGINEER by default (engineer unmutes selectively).
                -- Matches create_sends_for_member.lua:35-38 convention.
                if ie.name:lower():find("engineer") then
                    reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "B_MUTE", 1)
                end
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
