-- switch_listen_target.lua
-- Switch engineer listen target by muting/unmuting sends TO the ENGINEER inear track
-- Action ID: _RS_REAPERIEM_SWITCH_LISTEN
--
-- Reads EXTSTATE reaperiem/listen_target for target member name (e.g., "PETRONELA")
-- Finds ENGINEER inear track, iterates sends FROM member inear tracks TO engineer,
-- unmutes only the matching member's send, mutes all others.
--
-- Special target "ALL" unmutes all sends (restores normal engineer mix).
--
-- Result written to EXTSTATE reaperiem/listen_result:
--   OK:<member_name>     — switched successfully
--   OK:ALL               — all sends restored
--   FAIL:<reason>        — error

-- Find ENGINEER inear track
local function find_engineer_inear()
  local count = reaper.CountTracks(0)
  for i = 0, count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:upper():match("^ENGINEER%s+INEAR$") then
      return track, i
    end
  end
  return nil, -1
end

-- Main
local target = reaper.GetExtState("reaperiem", "listen_target")
if target == "" then
  reaper.SetExtState("reaperiem", "listen_result", "FAIL:no_target_set", false)
  return
end

local eng_track, _ = find_engineer_inear()
if not eng_track then
  reaper.SetExtState("reaperiem", "listen_result", "FAIL:engineer_inear_not_found", false)
  return
end

-- Iterate all tracks, find member inear tracks that have a send to ENGINEER inear
local track_count = reaper.CountTracks(0)
local matched = false
local restore_all = (target:upper() == "ALL")

for t = 0, track_count - 1 do
  local track = reaper.GetTrack(0, t)
  local _, track_name = reaper.GetTrackName(track)

  -- Only process member inear tracks (not ENGINEER inear itself)
  local member_name = track_name:match("^(%S+)%s+inear$")
  if member_name and member_name:upper() ~= "ENGINEER" then
    -- Find this track's send to ENGINEER inear
    local send_count = reaper.GetTrackNumSends(track, 0) -- 0 = sends
    for s = 0, send_count - 1 do
      local dest = reaper.GetTrackSendInfo_Value(track, 0, s, "P_DESTTRACK")
      if dest == eng_track then
        if restore_all then
          -- Unmute all sends (restore normal mix)
          reaper.SetTrackSendInfo_Value(track, 0, s, "B_MUTE", 0)
        elseif member_name:upper() == target:upper() then
          -- Unmute matching member's send
          reaper.SetTrackSendInfo_Value(track, 0, s, "B_MUTE", 0)
          matched = true
        else
          -- Mute all other member sends
          reaper.SetTrackSendInfo_Value(track, 0, s, "B_MUTE", 1)
        end
        break -- Each member inear has at most one send to engineer
      end
    end
  end
end

if restore_all then
  reaper.SetExtState("reaperiem", "listen_result", "OK:ALL", false)
elseif matched then
  reaper.SetExtState("reaperiem", "listen_result", "OK:" .. target, false)
else
  reaper.SetExtState("reaperiem", "listen_result", "FAIL:target_not_found:" .. target, false)
end
