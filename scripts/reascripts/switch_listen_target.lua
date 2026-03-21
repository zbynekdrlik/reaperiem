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
-- Mute state preservation:
--   On ListenStart: saves current mute states to EXTSTATE reaperiem/listen_mute_backup
--   On ListenStop:  restores mute states from backup, then clears backup
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

-- Collect all member inear sends to engineer: returns list of {track, name, send_idx, mute_state}
local function collect_member_sends(eng_track)
  local sends = {}
  local track_count = reaper.CountTracks(0)
  for t = 0, track_count - 1 do
    local track = reaper.GetTrack(0, t)
    local _, track_name = reaper.GetTrackName(track)
    local member_name = track_name:match("^(%S+)%s+inear$")
    if member_name and member_name:upper() ~= "ENGINEER" then
      local send_count = reaper.GetTrackNumSends(track, 0)
      for s = 0, send_count - 1 do
        local dest = reaper.GetTrackSendInfo_Value(track, 0, s, "P_DESTTRACK")
        if dest == eng_track then
          local muted = reaper.GetTrackSendInfo_Value(track, 0, s, "B_MUTE")
          table.insert(sends, {
            track = track,
            name = member_name:upper(),
            send_idx = s,
            mute_state = muted
          })
          break
        end
      end
    end
  end
  return sends
end

-- Save mute states as EXTSTATE: "NAME1:0;NAME2:1;NAME3:0"
local function save_mute_backup(sends)
  local parts = {}
  for _, s in ipairs(sends) do
    table.insert(parts, s.name .. ":" .. tostring(math.floor(s.mute_state)))
  end
  reaper.SetExtState("reaperiem", "listen_mute_backup", table.concat(parts, ";"), false)
end

-- Parse mute backup EXTSTATE into {NAME = mute_value} table
local function parse_mute_backup()
  local backup_str = reaper.GetExtState("reaperiem", "listen_mute_backup")
  if backup_str == "" then return nil end
  local result = {}
  for entry in backup_str:gmatch("[^;]+") do
    local name, val = entry:match("^(%S+):(%d+)$")
    if name and val then
      result[name:upper()] = tonumber(val)
    end
  end
  return result
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

local sends = collect_member_sends(eng_track)
local restore_all = (target:upper() == "ALL")

if restore_all then
  -- ListenStop: restore mute states from backup
  local backup = parse_mute_backup()
  if backup then
    for _, s in ipairs(sends) do
      local saved = backup[s.name]
      if saved then
        reaper.SetTrackSendInfo_Value(s.track, 0, s.send_idx, "B_MUTE", saved)
      else
        -- Member not in backup (new member added since listen started) — unmute
        reaper.SetTrackSendInfo_Value(s.track, 0, s.send_idx, "B_MUTE", 0)
      end
    end
  else
    -- No backup exists (edge case) — fall back to unmute all
    for _, s in ipairs(sends) do
      reaper.SetTrackSendInfo_Value(s.track, 0, s.send_idx, "B_MUTE", 0)
    end
  end
  -- Clear backup
  reaper.SetExtState("reaperiem", "listen_mute_backup", "", false)
  reaper.SetExtState("reaperiem", "listen_result", "OK:ALL", false)
else
  -- ListenStart: save current mute states if no backup exists yet
  local existing_backup = reaper.GetExtState("reaperiem", "listen_mute_backup")
  if existing_backup == "" then
    save_mute_backup(sends)
  end

  -- Mute all except target
  local matched = false
  for _, s in ipairs(sends) do
    if s.name == target:upper() then
      reaper.SetTrackSendInfo_Value(s.track, 0, s.send_idx, "B_MUTE", 0)
      matched = true
    else
      reaper.SetTrackSendInfo_Value(s.track, 0, s.send_idx, "B_MUTE", 1)
    end
  end

  if matched then
    reaper.SetExtState("reaperiem", "listen_result", "OK:" .. target, false)
  else
    reaper.SetExtState("reaperiem", "listen_result", "FAIL:target_not_found:" .. target, false)
  end
end
