-- IEM Project Setup Script
-- Creates the complete REAPER project structure for IEM mixing.
--
-- This script is meant to be run manually inside REAPER on iem.lan.
-- It will:
--   1. Clear all existing tracks (with user confirmation)
--   2. Create the full folder hierarchy (INPUTS > MICS/STEMS/TECH, OUTPUTS > BAND/TECH)
--   3. Create all input tracks with hardware (ASIO/Dante) inputs
--   4. Create all output tracks with hardware outputs
--   5. Wire 252 sends (28 inputs x 9 band outputs) plus 1 send for TRANSLATOR
--   6. Configure ENGINEER inear to receive the solo bus
--   7. Disable master send on all output tracks (so solo doesn't bleed to outputs)
--
-- Timing reference (design doc): 2026-02-23-reaper-iem-system-design.md

-- Auto-mode detection (for HTTP API triggering without dialogs)
local AUTO_MODE = reaper.GetExtState("reaperiem", "auto_setup") == "1"
if AUTO_MODE then
    reaper.SetExtState("reaperiem", "auto_setup", "0", false)  -- Clear flag immediately
end

-- ============================================================================
-- CONFIGURATION
-- ============================================================================

-- Input tracks: { name, dante_rx_channel (1-based) }
-- Grouped by folder.

local INPUT_MICS = {
    { name = "PETKA mic",    dante_rx = 3 },
    { name = "STEVO mic",    dante_rx = 4 },
    { name = "MAREK mic",    dante_rx = 5 },
    { name = "ZUZKA mic",    dante_rx = 6 },
    { name = "ZUZKA gtr",    dante_rx = 7 },
    { name = "TINA mic",     dante_rx = 8 },
    { name = "MIREC mic",    dante_rx = 9 },
    { name = "ALEX mic",     dante_rx = 10 },
    { name = "PATRIKA mic",  dante_rx = 11 },
    { name = "ANI mic",      dante_rx = 12 },
}

local INPUT_STEMS = {
    { name = "DRUMS L",      dante_rx = 21 },
    { name = "DRUMS R",      dante_rx = 22 },
    { name = "BASS L",       dante_rx = 23 },
    { name = "BASS R",       dante_rx = 24 },
    { name = "INST L",       dante_rx = 25 },
    { name = "INST R",       dante_rx = 26 },
    { name = "OTHER L",      dante_rx = 27 },
    { name = "OTHER R",      dante_rx = 28 },
    { name = "BGVS L",       dante_rx = 29 },
    { name = "BGVS R",       dante_rx = 30 },
    { name = "CLICK",        dante_rx = 31 },
    { name = "GUIDE",        dante_rx = 32 },
    { name = "IEMONLY L",    dante_rx = 33 },
    { name = "IEMONLY R",    dante_rx = 34 },
}

local INPUT_TECH = {
    { name = "HAND1 mic",    dante_rx = 49 },
    { name = "HAND2 mic",    dante_rx = 50 },
    { name = "HAND3 mic",    dante_rx = 51 },
    { name = "ENGINEER mic", dante_rx = 52 },
}

-- Output tracks: { name, dante_tx_l (1-based), dante_tx_r (1-based or nil for mono) }

local OUTPUT_BAND = {
    { name = "PETKA inear",   dante_tx_l = 3,  dante_tx_r = 4 },
    { name = "STEVO inear",   dante_tx_l = 5,  dante_tx_r = 6 },
    { name = "MAREK inear",   dante_tx_l = 7,  dante_tx_r = 8 },
    { name = "ZUZKA inear",   dante_tx_l = 9,  dante_tx_r = 10 },
    { name = "TINA inear",    dante_tx_l = 11, dante_tx_r = 12 },
    { name = "MIREC inear",   dante_tx_l = 13, dante_tx_r = 14 },
    { name = "ALEX inear",    dante_tx_l = 15, dante_tx_r = 16 },
    { name = "PATRIKA inear", dante_tx_l = 17, dante_tx_r = 18 },
    { name = "ANI inear",     dante_tx_l = 19, dante_tx_r = 20 },
}

local OUTPUT_TECH = {
    { name = "ENGINEER inear", dante_tx_l = 33, dante_tx_r = 34, solo_bus = true },
    { name = "TRANSLATOR",     dante_tx_l = 35, dante_tx_r = nil, mono = true },
}

-- ============================================================================
-- HELPERS
-- ============================================================================

-- Log a message to the REAPER console.
local function log(msg)
    reaper.ShowConsoleMsg(msg .. "\n")
end

-- Create a track at a given 0-based index and set its name.
-- Returns the MediaTrack pointer.
local function create_track(index, name)
    reaper.InsertTrackAtIndex(index, false) -- false = no defaults
    local track = reaper.GetTrack(0, index)
    reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
    -- Disarm recording by default
    reaper.SetMediaTrackInfo_Value(track, "I_RECARM", 0)
    return track
end

-- Make a track a folder parent.
-- folder_mode: 1 = folder start, 0 = normal, 2 = end of folder (last child)
local function set_folder_depth(track, folder_mode)
    reaper.SetMediaTrackInfo_Value(track, "I_FOLDERDEPTH", folder_mode)
end

-- Set mono hardware input on a track.
-- dante_rx is 1-based channel number.
local function set_hw_input_mono(track, dante_rx)
    -- I_RECINPUT: (channel index 0-based) | (mode flags)
    -- Mode 0 = normal mono input, add 1024 for MIDI, etc.
    -- For mono input: just the 0-based channel index
    local rec_input = (dante_rx - 1)  -- 0-based, mono
    reaper.SetMediaTrackInfo_Value(track, "I_RECINPUT", rec_input)
    -- Enable record monitoring so input can pass through
    reaper.SetMediaTrackInfo_Value(track, "I_RECMON", 1)
end

-- Remove all existing hardware outputs from a track.
local function clear_hw_outputs(track)
    local count = reaper.GetTrackNumSends(track, 1) -- category 1 = hw outputs
    for i = count - 1, 0, -1 do
        reaper.RemoveTrackSend(track, 1, i)
    end
end

-- Add a stereo hardware output pair.
-- ch_l, ch_r are 1-based channel numbers.
local function add_hw_output_stereo(track, ch_l, ch_r)
    clear_hw_outputs(track)
    local hw_idx = reaper.CreateTrackSend(track, nil) -- nil = hw output
    if hw_idx < 0 then
        log("  ERROR: failed to create hw output for track")
        return
    end
    -- I_DSTCHAN: 0-based left channel. Stereo is default (no &1024 flag).
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "I_DSTCHAN", ch_l - 1)
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "D_VOL", 1.0)
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "D_PAN", 0.0)
end

-- Add a mono hardware output.
-- ch is a 1-based channel number.
local function add_hw_output_mono(track, ch)
    clear_hw_outputs(track)
    local hw_idx = reaper.CreateTrackSend(track, nil)
    if hw_idx < 0 then
        log("  ERROR: failed to create mono hw output for track")
        return
    end
    -- For mono output: set I_DSTCHAN with bit 10 set (&1024) to indicate mono
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "I_DSTCHAN", (ch - 1) + 1024)
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "D_VOL", 1.0)
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "D_PAN", 0.0)
end

-- Create a send from src track to dst track. Returns the send index.
-- send_vol: linear volume (1.0 = 0dB).
local function create_send(src, dst, send_vol)
    local send_idx = reaper.CreateTrackSend(src, dst)
    if send_idx < 0 then
        log("  ERROR: failed to create send")
        return -1
    end
    reaper.SetTrackSendInfo_Value(src, 0, send_idx, "D_VOL", send_vol or 1.0)
    reaper.SetTrackSendInfo_Value(src, 0, send_idx, "D_PAN", 0.0)
    return send_idx
end

-- Disable the master/parent send on a track (direct routing only).
local function disable_master_send(track)
    reaper.SetMediaTrackInfo_Value(track, "B_MAINSEND", 0)
end

-- ============================================================================
-- MAIN SETUP
-- ============================================================================

local function main()
    if not AUTO_MODE then
        -- Confirmation dialog (only when run manually)
        local response = reaper.ShowMessageBox(
            "This will DELETE ALL existing tracks and create the full IEM project structure.\n\n" ..
            "Tracks: 28 inputs + 11 outputs = 39 tracks\n" ..
            "Sends: 252 (band) + 1 (translator) = 253 sends\n\n" ..
            "Continue?",
            "IEM Project Setup",
            1  -- OK/Cancel
        )
        if response ~= 1 then
            log("Setup cancelled by user.")
            return
        end
    else
        log("Auto-confirm mode (triggered via API)")
    end

    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    log("========================================")
    log("IEM Project Setup - Starting")
    log("========================================")

    -- Step 1: Remove all existing tracks
    local existing = reaper.CountTracks(0)
    log(string.format("Removing %d existing tracks...", existing))
    for i = existing - 1, 0, -1 do
        local t = reaper.GetTrack(0, i)
        if t then reaper.DeleteTrack(t) end
    end

    -- Track index counter (0-based, incremented as we create tracks)
    local idx = 0

    -- We will collect references for routing later.
    local input_tracks = {}   -- all 28 input tracks (MediaTrack pointers)
    local band_tracks = {}    -- 9 band output tracks
    local engineer_track = nil
    local translator_track = nil
    local hand1_track = nil   -- we need a reference to HAND1 for the translator send

    -- ========================================================================
    -- Step 2: INPUT tracks
    -- ========================================================================
    log("")
    log("--- Creating INPUT tracks ---")

    -- INPUTS folder parent
    local inputs_folder = create_track(idx, "INPUTS")
    set_folder_depth(inputs_folder, 1) -- folder start
    disable_master_send(inputs_folder) -- folder itself does not need master
    idx = idx + 1

    -- ---- MICS sub-folder ----
    log("  MICS folder (" .. #INPUT_MICS .. " tracks)")
    local mics_folder = create_track(idx, "MICS")
    set_folder_depth(mics_folder, 1)
    idx = idx + 1

    for i, mic in ipairs(INPUT_MICS) do
        local t = create_track(idx, mic.name)
        set_hw_input_mono(t, mic.dante_rx)
        input_tracks[#input_tracks + 1] = t
        -- Last child closes the folder
        if i == #INPUT_MICS then
            set_folder_depth(t, -1) -- close MICS folder
        end
        log("    " .. mic.name .. " (Dante RX " .. mic.dante_rx .. ")")
        idx = idx + 1
    end

    -- ---- STEMS sub-folder ----
    log("  STEMS folder (" .. #INPUT_STEMS .. " tracks)")
    local stems_folder = create_track(idx, "STEMS")
    set_folder_depth(stems_folder, 1)
    idx = idx + 1

    for i, stem in ipairs(INPUT_STEMS) do
        local t = create_track(idx, stem.name)
        set_hw_input_mono(t, stem.dante_rx)
        input_tracks[#input_tracks + 1] = t
        if i == #INPUT_STEMS then
            set_folder_depth(t, -1) -- close STEMS folder
        end
        log("    " .. stem.name .. " (Dante RX " .. stem.dante_rx .. ")")
        idx = idx + 1
    end

    -- ---- TECH (input) sub-folder ----
    log("  TECH input folder (" .. #INPUT_TECH .. " tracks)")
    local tech_in_folder = create_track(idx, "TECH")
    set_folder_depth(tech_in_folder, 1)
    idx = idx + 1

    for i, tech in ipairs(INPUT_TECH) do
        local t = create_track(idx, tech.name)
        set_hw_input_mono(t, tech.dante_rx)
        input_tracks[#input_tracks + 1] = t
        -- Remember HAND1 for translator routing
        if tech.name == "HAND1 mic" then
            hand1_track = t
        end
        if i == #INPUT_TECH then
            set_folder_depth(t, -2) -- close TECH folder AND INPUTS folder
        end
        log("    " .. tech.name .. " (Dante RX " .. tech.dante_rx .. ")")
        idx = idx + 1
    end

    log(string.format("  Total input tracks: %d", #input_tracks))

    -- ========================================================================
    -- Step 3: OUTPUT tracks
    -- ========================================================================
    log("")
    log("--- Creating OUTPUT tracks ---")

    -- OUTPUTS folder parent
    local outputs_folder = create_track(idx, "OUTPUTS")
    set_folder_depth(outputs_folder, 1)
    disable_master_send(outputs_folder)
    idx = idx + 1

    -- ---- BAND sub-folder ----
    log("  BAND folder (" .. #OUTPUT_BAND .. " tracks)")
    local band_folder = create_track(idx, "BAND")
    set_folder_depth(band_folder, 1)
    disable_master_send(band_folder)
    idx = idx + 1

    for i, member in ipairs(OUTPUT_BAND) do
        local t = create_track(idx, member.name)
        -- Disable master send so solo routing does not affect band outputs
        disable_master_send(t)
        -- Set stereo hardware output
        add_hw_output_stereo(t, member.dante_tx_l, member.dante_tx_r)
        band_tracks[#band_tracks + 1] = t
        if i == #OUTPUT_BAND then
            set_folder_depth(t, -1) -- close BAND folder
        end
        log("    " .. member.name .. " (Dante TX " .. member.dante_tx_l .. "-" .. member.dante_tx_r .. ")")
        idx = idx + 1
    end

    -- ---- TECH (output) sub-folder ----
    log("  TECH output folder (" .. #OUTPUT_TECH .. " tracks)")
    local tech_out_folder = create_track(idx, "TECH")
    set_folder_depth(tech_out_folder, 1)
    disable_master_send(tech_out_folder)
    idx = idx + 1

    for i, tech in ipairs(OUTPUT_TECH) do
        local t = create_track(idx, tech.name)
        disable_master_send(t)

        if tech.mono then
            add_hw_output_mono(t, tech.dante_tx_l)
            log("    " .. tech.name .. " (Dante TX " .. tech.dante_tx_l .. " mono)")
        else
            add_hw_output_stereo(t, tech.dante_tx_l, tech.dante_tx_r)
            log("    " .. tech.name .. " (Dante TX " .. tech.dante_tx_l .. "-" .. tech.dante_tx_r .. ")")
        end

        if tech.name == "ENGINEER inear" then
            engineer_track = t
        elseif tech.name == "TRANSLATOR" then
            translator_track = t
        end

        if i == #OUTPUT_TECH then
            set_folder_depth(t, -2) -- close TECH folder AND OUTPUTS folder
        end
        idx = idx + 1
    end

    -- ========================================================================
    -- Step 4: Create sends (28 inputs -> 9 band outputs = 252 sends)
    -- ========================================================================
    log("")
    log("--- Creating sends ---")

    local send_count = 0
    for _, src in ipairs(input_tracks) do
        for _, dst in ipairs(band_tracks) do
            create_send(src, dst, 1.0)  -- 0dB default
            send_count = send_count + 1
        end
    end
    log(string.format("  Band sends created: %d (expected 252)", send_count))

    -- TRANSLATOR: receives only HAND1 mic
    if hand1_track and translator_track then
        create_send(hand1_track, translator_track, 1.0)
        send_count = send_count + 1
        log("  TRANSLATOR send created: HAND1 mic -> TRANSLATOR")
    else
        log("  WARNING: could not create TRANSLATOR send (missing track references)")
    end

    log(string.format("  Total sends: %d (expected 253)", send_count))

    -- ========================================================================
    -- Step 5: Configure ENGINEER solo bus
    -- ========================================================================
    log("")
    log("--- Configuring ENGINEER solo bus ---")

    if engineer_track then
        -- In REAPER, to route the solo bus to a specific track:
        -- 1. The track must receive the solo-in-place bus
        -- 2. Set B_SOLO_DEFEAT = 1 so the track itself is not affected by solo
        -- 3. Enable "Listen to solo in this track" via I_SOLO_FLAGS
        --
        -- REAPER's solo routing works through the master/monitor bus.
        -- The cleanest approach: ENGINEER receives a send from the Master track,
        -- and we set the master to output to ENGINEER when solo is active.
        --
        -- Actually, the simplest approach in REAPER is:
        -- - Set the project solo mode to "SIP" (solo in place)
        -- - Route the master/monitor output to ENGINEER
        -- - ENGINEER hears whatever goes through the master bus (which reflects solo)
        --
        -- For now, we set solo defeat on the ENGINEER track and create a send
        -- from the master track to it. The engineer will hear the master bus content
        -- which changes based on solo state.

        local master = reaper.GetMasterTrack(0)
        if master then
            local send_idx = reaper.CreateTrackSend(master, engineer_track)
            if send_idx >= 0 then
                reaper.SetTrackSendInfo_Value(master, 0, send_idx, "D_VOL", 1.0)
                reaper.SetTrackSendInfo_Value(master, 0, send_idx, "D_PAN", 0.0)
                log("  Master -> ENGINEER inear send created")
            end
        end

        -- Solo defeat: when other tracks are solo'd, ENGINEER track keeps playing
        reaper.SetMediaTrackInfo_Value(engineer_track, "B_SOLO_DEFEAT", 1)
        log("  ENGINEER solo defeat enabled")
    else
        log("  WARNING: ENGINEER track not found, skipping solo bus config")
    end

    -- ========================================================================
    -- Step 6: Set solo defeat on all output tracks
    -- ========================================================================
    -- All output tracks should be unaffected by solo (they have their own sends).
    log("")
    log("--- Setting solo defeat on output tracks ---")

    for _, t in ipairs(band_tracks) do
        reaper.SetMediaTrackInfo_Value(t, "B_SOLO_DEFEAT", 1)
    end
    if translator_track then
        reaper.SetMediaTrackInfo_Value(translator_track, "B_SOLO_DEFEAT", 1)
    end
    log("  Solo defeat set on all " .. (#band_tracks + 2) .. " output tracks")

    -- ========================================================================
    -- Done
    -- ========================================================================
    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("IEM Project Setup", -1)

    log("")
    log("========================================")
    log("IEM Project Setup - COMPLETE")
    log("========================================")
    log(string.format("  Tracks: %d total (%d inputs + %d outputs + folder tracks)",
        idx, #input_tracks, #OUTPUT_BAND + #OUTPUT_TECH))
    log(string.format("  Sends: %d", send_count))
    log("")
    log("Next steps:")
    log("  1. Verify track names and routing in REAPER mixer view")
    log("  2. Check hardware outputs in routing matrix (Ctrl+Alt+R)")
    log("  3. Save project: Ctrl+S")
    log("  4. Commit on iem.lan: git add -A && git commit -m 'feat: initial IEM project'")

    if not AUTO_MODE then
        reaper.ShowMessageBox(
            "IEM project setup complete!\n\n" ..
            "Tracks: " .. idx .. "\n" ..
            "Sends: " .. send_count .. "\n\n" ..
            "Check the console for details.\n" ..
            "Verify routing in the mixer and routing matrix (Ctrl+Alt+R).",
            "Setup Complete",
            0  -- OK only
        )
    end
end

-- Run with error handling
local ok, err = pcall(main)
if not ok then
    log("FATAL ERROR: " .. tostring(err))
    if not AUTO_MODE then
        reaper.ShowMessageBox("Setup failed!\n\n" .. tostring(err), "Error", 0)
    end
end
