-- setup_monitor_bus.lua
-- Create MONITOR bus track with sends from all member inear tracks
-- Action ID: _RS_REAPERIEM_SETUP_MONITOR
-- Usage: One-time setup, idempotent (skips if MONITOR bus exists)
--
-- Architecture:
--   Member inear tracks --send--> MONITOR bus --VBAN VST3--> UDP :6980 --> app --> browser
--   Only ONE send is unmuted at a time (switched by switch_listen_target.lua)
--
-- If VBAN IEM VST3 is on ENGINEER inear, it will be moved to MONITOR bus.
-- All sends to MONITOR bus start muted.
-- MONITOR bus has no hardware output (audio only via VBAN).
--
-- Result written to EXTSTATE reaperiem/monitor_setup_result:
--   OK:<track_idx>:<send_count>  — created successfully
--   SKIP:<track_idx>             — already exists
--   FAIL:<reason>                — error

-- Find track by name (case-insensitive partial match)
local function find_track(name_pattern)
  local count = reaper.CountTracks(0)
  for i = 0, count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:lower():match(name_pattern:lower()) then
      return track, i
    end
  end
  return nil, -1
end

-- Find all inear tracks
local function find_inear_tracks()
  local tracks = {}
  local count = reaper.CountTracks(0)
  for i = 0, count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:lower():match("inear$") then
      table.insert(tracks, { track = track, index = i, name = name })
    end
  end
  return tracks
end

-- Check if VBAN IEM is on a track, return fx index
local function find_vban_fx(track)
  local fx_count = reaper.TrackFX_GetCount(track)
  for i = 0, fx_count - 1 do
    local _, fx_name = reaper.TrackFX_GetFXName(track, i)
    if fx_name:lower():match("vban iem") then
      return i
    end
  end
  return nil
end

-- Check if a send already exists from source to dest
local function send_exists(source, dest)
  local send_count = reaper.GetTrackNumSends(source, 0) -- 0 = sends
  for i = 0, send_count - 1 do
    local dest_track = reaper.GetTrackSendInfo_Value(source, 0, i, "P_DESTTRACK")
    if dest_track == dest then
      return true, i
    end
  end
  return false, -1
end

-- Main
local monitor, monitor_idx = find_track("^monitor bus$")

if monitor then
  -- Already exists
  reaper.SetExtState("reaperiem", "monitor_setup_result", "SKIP:" .. (monitor_idx + 1), false)
  return
end

-- Create MONITOR bus track at the end
reaper.InsertTrackAtIndex(reaper.CountTracks(0), true)
monitor_idx = reaper.CountTracks(0) - 1
monitor = reaper.GetTrack(0, monitor_idx)
reaper.GetSetMediaTrackInfo_String(monitor, "P_NAME", "MONITOR bus", true)

-- Remove hardware output from MONITOR bus (audio only through VBAN)
-- Set main send off (I_MAINSEND = 0 means no master/parent send)
reaper.SetMediaTrackInfo_Value(monitor, "I_MAINSEND", 0)

-- Move VBAN IEM from ENGINEER inear to MONITOR bus (if present)
local eng_track = find_track("engineer.*inear")
if eng_track then
  local vban_idx = find_vban_fx(eng_track)
  if vban_idx then
    -- Copy FX to MONITOR bus then delete from engineer
    reaper.TrackFX_CopyToTrack(eng_track, vban_idx, monitor, 0, true) -- true = move
  end
end

-- If VBAN wasn't moved, insert fresh one on MONITOR bus
if not find_vban_fx(monitor) then
  local new_fx = reaper.TrackFX_AddByName(monitor, "VBAN IEM", false, -1)
  if new_fx < 0 then
    reaper.SetExtState("reaperiem", "monitor_setup_result", "FAIL:vban_insert_failed", false)
    return
  end
end

-- Add sends from all inear tracks to MONITOR bus
local inear_tracks = find_inear_tracks()
local send_count = 0

for _, info in ipairs(inear_tracks) do
  -- Skip if send already exists
  local exists = send_exists(info.track, monitor)
  if not exists then
    local send_idx = reaper.CreateTrackSend(info.track, monitor)
    if send_idx >= 0 then
      -- Mute the send (all start muted; switch_listen_target unmutes one)
      reaper.SetTrackSendInfo_Value(info.track, 0, send_idx, "B_MUTE", 1)
      -- Set send to post-fader so engineer hears actual member volume
      reaper.SetTrackSendInfo_Value(info.track, 0, send_idx, "I_SENDMODE", 0) -- 0 = post-fader post-fx
      send_count = send_count + 1
    end
  else
    send_count = send_count + 1
  end
end

reaper.SetExtState("reaperiem", "monitor_setup_result",
  "OK:" .. (monitor_idx + 1) .. ":" .. send_count, false)
