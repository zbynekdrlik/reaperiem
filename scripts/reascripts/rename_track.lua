-- Rename Track via HTTP API
-- Called via HTTP API with parameters passed through ExtState
--
-- ExtState keys (section: reaperiem):
--   rename_track_index: 1-based track index
--   rename_track_name: New track name

local section = "reaperiem"

-- Read parameters from ExtState
local track_index = tonumber(reaper.GetExtState(section, "rename_track_index"))
local new_name = reaper.GetExtState(section, "rename_track_name")

-- Validate parameters
if not track_index then
    reaper.ShowConsoleMsg("Error: Missing track index in ExtState\n")
    reaper.SetExtState(section, "rename_result", "error: missing track index", false)
    return
end

if not new_name or new_name == "" then
    reaper.ShowConsoleMsg("Error: Missing track name in ExtState\n")
    reaper.SetExtState(section, "rename_result", "error: missing track name", false)
    return
end

-- Get the track (convert 1-based to 0-based for API)
local track = reaper.GetTrack(0, track_index - 1)
if not track then
    reaper.ShowConsoleMsg("Error: Track " .. track_index .. " not found\n")
    reaper.SetExtState(section, "rename_result", "error: track not found", false)
    return
end

-- Get old name for logging
local _, old_name = reaper.GetTrackName(track)

-- Rename the track
local success = reaper.GetSetMediaTrackInfo_String(track, "P_NAME", new_name, true)

if success then
    local msg = string.format("Renamed track %d: '%s' -> '%s'", track_index, old_name, new_name)
    reaper.ShowConsoleMsg(msg .. "\n")
    reaper.SetExtState(section, "rename_result", "success: " .. msg, false)

    -- Create undo point
    reaper.Undo_OnStateChangeEx("Rename track to " .. new_name, -1, -1)
else
    reaper.ShowConsoleMsg("Error: Failed to rename track\n")
    reaper.SetExtState(section, "rename_result", "error: rename failed", false)
end

-- Clear the input parameters
reaper.SetExtState(section, "rename_track_index", "", false)
reaper.SetExtState(section, "rename_track_name", "", false)
