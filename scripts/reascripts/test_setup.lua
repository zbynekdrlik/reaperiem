-- Test script for debugging
reaper.ShowConsoleMsg("=== TEST SCRIPT STARTED ===\n")

-- Test 1: Insert a track
reaper.ShowConsoleMsg("Test 1: Inserting track...\n")
reaper.InsertTrackAtIndex(0, false)
local track = reaper.GetTrack(0, 0)
if track then
    reaper.ShowConsoleMsg("  Track created successfully\n")
    reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "TEST TRACK", true)
else
    reaper.ShowConsoleMsg("  ERROR: Failed to create track\n")
end

-- Test 2: Set folder depth
reaper.ShowConsoleMsg("Test 2: Setting folder depth...\n")
local ok, err = pcall(function()
    reaper.SetMediaTrackInfo_Value(track, "I_FOLDERDEPTH", 1)
end)
if ok then
    reaper.ShowConsoleMsg("  Folder depth set\n")
else
    reaper.ShowConsoleMsg("  ERROR: " .. tostring(err) .. "\n")
end

-- Test 3: Create another track and a send
reaper.ShowConsoleMsg("Test 3: Creating send...\n")
reaper.InsertTrackAtIndex(1, false)
local track2 = reaper.GetTrack(0, 1)
if track2 then
    reaper.GetSetMediaTrackInfo_String(track2, "P_NAME", "TEST TRACK 2", true)
    local send_idx = reaper.CreateTrackSend(track, track2)
    if send_idx >= 0 then
        reaper.ShowConsoleMsg("  Send created: idx " .. send_idx .. "\n")
    else
        reaper.ShowConsoleMsg("  ERROR: Failed to create send\n")
    end
end

reaper.ShowConsoleMsg("=== TEST COMPLETE ===\n")
