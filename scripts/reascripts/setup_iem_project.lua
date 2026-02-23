-- IEM Project Setup Script
-- Creates the complete REAPER project structure for IEM mixing.
--
-- This script can be run manually OR triggered via HTTP API.
-- When triggered via API with auto_setup=1, all dialogs are skipped.
--
-- It will:
--   1. Clear all existing tracks (with user confirmation if manual)
--   2. Create the full folder hierarchy (INPUTS > MICS/STEMS/TECH, OUTPUTS > BAND/TECH)
--   3. Create all input tracks with hardware (ASIO/Dante) inputs
--   4. Create all output tracks with hardware outputs
--   5. Wire 252 sends (28 inputs x 9 band outputs) plus 1 send for TRANSLATOR
--   6. Configure ENGINEER inear to receive the solo bus
--   7. Disable master send on all output tracks

-- Auto-mode detection (for HTTP API triggering without dialogs)
local AUTO_MODE = reaper.GetExtState("reaperiem", "auto_setup") == "1"
if AUTO_MODE then
    reaper.SetExtState("reaperiem", "auto_setup", "0", false)  -- Clear flag immediately
end

-- ============================================================================
-- CONFIGURATION
-- ============================================================================

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

local function log(msg)
    reaper.ShowConsoleMsg(msg .. "\n")
end

local function create_track(index, name)
    reaper.InsertTrackAtIndex(index, false)
    local track = reaper.GetTrack(0, index)
    reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
    reaper.SetMediaTrackInfo_Value(track, "I_RECARM", 0)
    return track
end

local function set_folder_depth(track, folder_mode)
    reaper.SetMediaTrackInfo_Value(track, "I_FOLDERDEPTH", folder_mode)
end

local function set_hw_input_mono(track, dante_rx)
    local rec_input = (dante_rx - 1)
    reaper.SetMediaTrackInfo_Value(track, "I_RECINPUT", rec_input)
    reaper.SetMediaTrackInfo_Value(track, "I_RECMON", 1)
end

local function clear_hw_outputs(track)
    local count = reaper.GetTrackNumSends(track, 1)
    for i = count - 1, 0, -1 do
        reaper.RemoveTrackSend(track, 1, i)
    end
end

local function add_hw_output_stereo(track, ch_l, ch_r)
    clear_hw_outputs(track)
    local hw_idx = reaper.CreateTrackSend(track, nil)
    if hw_idx < 0 then
        log("  ERROR: failed to create hw output for track")
        return
    end
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "I_DSTCHAN", ch_l - 1)
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "D_VOL", 1.0)
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "D_PAN", 0.0)
end

local function add_hw_output_mono(track, ch)
    clear_hw_outputs(track)
    local hw_idx = reaper.CreateTrackSend(track, nil)
    if hw_idx < 0 then
        log("  ERROR: failed to create mono hw output for track")
        return
    end
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "I_DSTCHAN", (ch - 1) + 1024)
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "D_VOL", 1.0)
    reaper.SetTrackSendInfo_Value(track, 1, hw_idx, "D_PAN", 0.0)
end

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

local function disable_master_send(track)
    reaper.SetMediaTrackInfo_Value(track, "B_MAINSEND", 0)
end

-- ============================================================================
-- STATE (module level for deferred access)
-- ============================================================================

local input_tracks = {}
local band_tracks = {}
local engineer_track = nil
local translator_track = nil
local hand1_track = nil
local total_track_count = 0
local send_queue = {}
local send_index = 1
local send_count = 0

-- ============================================================================
-- PHASE 1: Create all tracks
-- ============================================================================

local function create_all_tracks()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    log("========================================")
    log("IEM Project Setup - Starting")
    log("========================================")

    -- Remove all existing tracks
    local existing = reaper.CountTracks(0)
    log(string.format("Removing %d existing tracks...", existing))
    for i = existing - 1, 0, -1 do
        local t = reaper.GetTrack(0, i)
        if t then reaper.DeleteTrack(t) end
    end

    local idx = 0

    -- ---- INPUTS folder ----
    log("")
    log("--- Creating INPUT tracks ---")

    local inputs_folder = create_track(idx, "INPUTS")
    set_folder_depth(inputs_folder, 1)
    disable_master_send(inputs_folder)
    idx = idx + 1

    -- MICS sub-folder
    log("  MICS folder (" .. #INPUT_MICS .. " tracks)")
    local mics_folder = create_track(idx, "MICS")
    set_folder_depth(mics_folder, 1)
    idx = idx + 1

    for i, mic in ipairs(INPUT_MICS) do
        local t = create_track(idx, mic.name)
        set_hw_input_mono(t, mic.dante_rx)
        input_tracks[#input_tracks + 1] = t
        if i == #INPUT_MICS then
            set_folder_depth(t, -1)
        end
        log("    " .. mic.name .. " (Dante RX " .. mic.dante_rx .. ")")
        idx = idx + 1
    end

    -- STEMS sub-folder
    log("  STEMS folder (" .. #INPUT_STEMS .. " tracks)")
    local stems_folder = create_track(idx, "STEMS")
    set_folder_depth(stems_folder, 1)
    idx = idx + 1

    for i, stem in ipairs(INPUT_STEMS) do
        local t = create_track(idx, stem.name)
        set_hw_input_mono(t, stem.dante_rx)
        input_tracks[#input_tracks + 1] = t
        if i == #INPUT_STEMS then
            set_folder_depth(t, -1)
        end
        log("    " .. stem.name .. " (Dante RX " .. stem.dante_rx .. ")")
        idx = idx + 1
    end

    -- TECH input sub-folder
    log("  TECH input folder (" .. #INPUT_TECH .. " tracks)")
    local tech_in_folder = create_track(idx, "TECH")
    set_folder_depth(tech_in_folder, 1)
    idx = idx + 1

    for i, tech in ipairs(INPUT_TECH) do
        local t = create_track(idx, tech.name)
        set_hw_input_mono(t, tech.dante_rx)
        input_tracks[#input_tracks + 1] = t
        if tech.name == "HAND1 mic" then
            hand1_track = t
        end
        if i == #INPUT_TECH then
            set_folder_depth(t, -2)
        end
        log("    " .. tech.name .. " (Dante RX " .. tech.dante_rx .. ")")
        idx = idx + 1
    end

    log(string.format("  Total input tracks: %d", #input_tracks))

    -- ---- OUTPUTS folder ----
    log("")
    log("--- Creating OUTPUT tracks ---")

    local outputs_folder = create_track(idx, "OUTPUTS")
    set_folder_depth(outputs_folder, 1)
    disable_master_send(outputs_folder)
    idx = idx + 1

    -- BAND sub-folder
    log("  BAND folder (" .. #OUTPUT_BAND .. " tracks)")
    local band_folder = create_track(idx, "BAND")
    set_folder_depth(band_folder, 1)
    disable_master_send(band_folder)
    idx = idx + 1

    for i, member in ipairs(OUTPUT_BAND) do
        local t = create_track(idx, member.name)
        disable_master_send(t)
        add_hw_output_stereo(t, member.dante_tx_l, member.dante_tx_r)
        band_tracks[#band_tracks + 1] = t
        if i == #OUTPUT_BAND then
            set_folder_depth(t, -1)
        end
        log("    " .. member.name .. " (Dante TX " .. member.dante_tx_l .. "-" .. member.dante_tx_r .. ")")
        idx = idx + 1
    end

    -- TECH output sub-folder
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
            set_folder_depth(t, -2)
        end
        idx = idx + 1
    end

    total_track_count = idx

    -- Refresh UI
    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()

    log("")
    log(string.format("Tracks created: %d", total_track_count))

    -- Build send queue
    for _, src in ipairs(input_tracks) do
        for _, dst in ipairs(band_tracks) do
            send_queue[#send_queue + 1] = { src = src, dst = dst }
        end
    end

    log(string.format("Queued %d sends for batch creation...", #send_queue))
    log("")

    -- Start batched send creation
    reaper.defer(process_send_batch)
end

-- ============================================================================
-- PHASE 2: Create sends in batches (deferred)
-- ============================================================================

local BATCH_SIZE = 25  -- sends per batch

function process_send_batch()
    local batch_end = math.min(send_index + BATCH_SIZE - 1, #send_queue)

    reaper.PreventUIRefresh(1)

    for i = send_index, batch_end do
        local pair = send_queue[i]
        create_send(pair.src, pair.dst, 1.0)
        send_count = send_count + 1
    end

    reaper.PreventUIRefresh(-1)

    send_index = batch_end + 1

    if send_index <= #send_queue then
        -- More to do - log progress and schedule next batch
        if send_count % 50 == 0 then
            log(string.format("  ... %d/%d sends created", send_count, #send_queue))
        end
        reaper.defer(process_send_batch)
    else
        -- All band sends done - continue with finalization
        log(string.format("  Band sends created: %d (expected 252)", send_count))
        reaper.defer(finalize_setup)
    end
end

-- ============================================================================
-- PHASE 3: Finalize setup
-- ============================================================================

function finalize_setup()
    log("")
    log("--- Finalizing setup ---")

    -- TRANSLATOR: receives only HAND1 mic
    if hand1_track and translator_track then
        create_send(hand1_track, translator_track, 1.0)
        send_count = send_count + 1
        log("  TRANSLATOR send created: HAND1 mic -> TRANSLATOR")
    else
        log("  WARNING: could not create TRANSLATOR send (missing track references)")
    end

    log(string.format("  Total sends: %d (expected 253)", send_count))

    -- Configure ENGINEER solo bus
    log("")
    log("--- Configuring ENGINEER solo bus ---")

    if engineer_track then
        local master = reaper.GetMasterTrack(0)
        if master then
            local send_idx = reaper.CreateTrackSend(master, engineer_track)
            if send_idx >= 0 then
                reaper.SetTrackSendInfo_Value(master, 0, send_idx, "D_VOL", 1.0)
                reaper.SetTrackSendInfo_Value(master, 0, send_idx, "D_PAN", 0.0)
                log("  Master -> ENGINEER inear send created")
            end
        end
        reaper.SetMediaTrackInfo_Value(engineer_track, "B_SOLO_DEFEAT", 1)
        log("  ENGINEER solo defeat enabled")
    else
        log("  WARNING: ENGINEER track not found, skipping solo bus config")
    end

    -- Set solo defeat on all output tracks
    log("")
    log("--- Setting solo defeat on output tracks ---")

    for _, t in ipairs(band_tracks) do
        reaper.SetMediaTrackInfo_Value(t, "B_SOLO_DEFEAT", 1)
    end
    if translator_track then
        reaper.SetMediaTrackInfo_Value(translator_track, "B_SOLO_DEFEAT", 1)
    end
    log("  Solo defeat set on all " .. (#band_tracks + 2) .. " output tracks")

    -- Final UI refresh
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("IEM Project Setup", -1)

    -- Done
    log("")
    log("========================================")
    log("IEM Project Setup - COMPLETE")
    log("========================================")
    log(string.format("  Tracks: %d", total_track_count))
    log(string.format("  Sends: %d", send_count))
    log("")
    log("Next steps:")
    log("  1. Verify track names and routing in REAPER mixer view")
    log("  2. Check hardware outputs in routing matrix (Ctrl+Alt+R)")
    log("  3. Save project: Ctrl+S")
    log("")

    if not AUTO_MODE then
        reaper.ShowMessageBox(
            "IEM project setup complete!\n\n" ..
            "Tracks: " .. total_track_count .. "\n" ..
            "Sends: " .. send_count .. "\n\n" ..
            "Check the console for details.",
            "Setup Complete",
            0
        )
    end
end

-- ============================================================================
-- MAIN ENTRY POINT
-- ============================================================================

local function main()
    if not AUTO_MODE then
        local response = reaper.ShowMessageBox(
            "This will DELETE ALL existing tracks and create the full IEM project structure.\n\n" ..
            "Tracks: 28 inputs + 11 outputs = 39 tracks\n" ..
            "Sends: 252 (band) + 1 (translator) = 253 sends\n\n" ..
            "Continue?",
            "IEM Project Setup",
            1
        )
        if response ~= 1 then
            log("Setup cancelled by user.")
            return
        end
    else
        log("Auto-confirm mode (triggered via API)")
    end

    create_all_tracks()
end

-- Run
local ok, err = pcall(main)
if not ok then
    log("FATAL ERROR: " .. tostring(err))
    if not AUTO_MODE then
        reaper.ShowMessageBox("Setup failed!\n\n" .. tostring(err), "Error", 0)
    end
end
