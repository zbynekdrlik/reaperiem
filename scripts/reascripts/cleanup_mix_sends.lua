-- cleanup_mix_sends.lua
-- Removes ONLY cross-member inear→inear sends (the ones causing routing loops).
-- Triggered via: _RS_REAPERIEM_CLEANUP_MIX_SENDS
--
-- Safety rules:
--   ONLY removes sends where BOTH source AND destination are non-engineer inear tracks
--   NEVER touches hardware output sends (dest track is nil / hardware)
--   NEVER touches sends to ENGINEER inear
--   NEVER touches sends on input tracks or stems tracks
--   Logs every removal for audit

local function log(msg)
    reaper.ShowConsoleMsg(msg .. "\n")
end

-- Build set of non-engineer inear tracks
local inear_tracks = {}  -- MediaTrack -> name
local track_count = reaper.CountTracks(0)
for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if (name:match(" inear$") or name:match(" INEAR$")) and not name:match("^ENGINEER") then
        inear_tracks[track] = name
    end
end

reaper.Undo_BeginBlock()
reaper.PreventUIRefresh(1)

local removed = 0
local skipped = 0

for src_track, src_name in pairs(inear_tracks) do
    local num_sends = reaper.GetTrackNumSends(src_track, 0)
    -- Iterate backwards for safe removal
    for s = num_sends - 1, 0, -1 do
        local dest = reaper.GetTrackSendInfo_Value(src_track, 0, s, "P_DESTTRACK")
        if dest and inear_tracks[dest] then
            -- Both source and dest are non-engineer inear tracks — safe to remove
            local dest_name = inear_tracks[dest]
            reaper.RemoveTrackSend(src_track, 0, s)
            removed = removed + 1
            log("Removed: " .. src_name .. " -> " .. dest_name .. " (send " .. s .. ")")
        else
            skipped = skipped + 1
        end
    end
end

reaper.PreventUIRefresh(-1)
reaper.Undo_EndBlock("Cleanup cross-member inear sends", -1)
reaper.Main_SaveProject(0, false)

local result = string.format("OK:removed=%d,skipped=%d", removed, skipped)
reaper.SetExtState("reaperiem", "cleanup_mix_sends_result", result, false)
log("Cleanup complete: " .. result)
