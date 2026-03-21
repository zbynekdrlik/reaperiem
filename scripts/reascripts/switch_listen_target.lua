-- switch_listen_target.lua
-- Switch engineer listen target by muting/unmuting sends on MONITOR bus
-- Action ID: _RS_REAPERIEM_SWITCH_LISTEN
--
-- Reads EXTSTATE reaperiem/listen_target for target member name (e.g., "PETRONELA", "ENGINEER")
-- Finds MONITOR bus track, iterates its receives, unmutes only the matching source.
--
-- Result written to EXTSTATE reaperiem/listen_result:
--   OK:<member_name>     — switched successfully
--   FAIL:<reason>        — error

-- Find MONITOR bus track
local function find_monitor_bus()
  local count = reaper.CountTracks(0)
  for i = 0, count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:lower() == "monitor bus" then
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

local monitor, _ = find_monitor_bus()
if not monitor then
  reaper.SetExtState("reaperiem", "listen_result", "FAIL:monitor_bus_not_found", false)
  return
end

-- Iterate receives on MONITOR bus (receives = category 1 in GetTrackNumSends)
-- For receives, the "source" is the sending track
local recv_count = reaper.GetTrackNumSends(monitor, -1) -- -1 = receives
local matched = false

for i = 0, recv_count - 1 do
  local src_track = reaper.GetTrackSendInfo_Value(monitor, -1, i, "P_SRCTRACK")
  if src_track then
    local _, src_name = reaper.GetTrackName(src_track)
    -- Match: source track name starts with target name and ends with "inear"
    local src_member = src_name:match("^(%S+)%s+inear$")
    if src_member and src_member:upper() == target:upper() then
      -- Unmute this receive
      reaper.SetTrackSendInfo_Value(monitor, -1, i, "B_MUTE", 0)
      matched = true
    else
      -- Mute all other receives
      reaper.SetTrackSendInfo_Value(monitor, -1, i, "B_MUTE", 1)
    end
  end
end

if matched then
  reaper.SetExtState("reaperiem", "listen_result", "OK:" .. target, false)
else
  reaper.SetExtState("reaperiem", "listen_result", "FAIL:target_not_found:" .. target, false)
end
