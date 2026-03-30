-- setup_talkback.lua
-- Create a TALKBACK track with OIEM Receive VST3 and sends to all member inear tracks
-- Action ID: _RS_REAPERIEM_SETUP_TALKBACK
-- Usage: One-time setup, idempotent (skips if OIEM Receive already present)
--
-- Creates a dedicated TALKBACK track (separate from ENGINEER mic),
-- inserts "OIEM Receive" VST3 on it, and creates pre-fader post-FX sends
-- to every band member inear track (excluding ENGINEER inear).
--
-- Result is written to EXTSTATE for remote verification:
--   reaper.GetExtState("reaperiem", "setup_talkback")
--   OK:<track_idx>:<fx_idx>:sends=<count>  — created successfully
--   SKIP:<track_idx>:<fx_idx>              — already present
--   FAIL:<reason>                          — error

-- Find existing TALKBACK track
local function find_talkback_track()
  local track_count = reaper.CountTracks(0)
  for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:upper() == "TALKBACK" then
      return track, i
    end
  end
  return nil, -1
end

-- Create a new TALKBACK track at the end of the project
local function create_talkback_track()
  local track_count = reaper.CountTracks(0)
  reaper.InsertTrackAtIndex(track_count, false)
  local track = reaper.GetTrack(0, track_count)
  if not track then
    return nil, -1
  end
  reaper.GetSetMediaTrackInfo_String(track, "P_NAME", "TALKBACK", true)
  return track, track_count
end

-- Check if OIEM Receive is already present on the track
local function has_oiem_receive(track)
  local fx_count = reaper.TrackFX_GetCount(track)
  for i = 0, fx_count - 1 do
    local _, fx_name = reaper.TrackFX_GetFXName(track, i)
    if fx_name:lower():match("oiem receive") then
      return true, i
    end
  end
  return false, -1
end

-- Find all member inear tracks (ending in " inear", excluding ENGINEER inear)
local function find_member_inear_tracks()
  local tracks = {}
  local track_count = reaper.CountTracks(0)
  for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:lower():match(" inear$") and not name:lower():match("engineer") then
      table.insert(tracks, { track = track, idx = i, name = name })
    end
  end
  return tracks
end

-- Check if a send from src_track to dst_track already exists
local function send_exists(src_track, dst_track)
  local send_count = reaper.GetTrackNumSends(src_track, 0) -- 0 = sends
  for i = 0, send_count - 1 do
    local dest = reaper.GetTrackSendInfo_Value(src_track, 0, i, "P_DESTTRACK")
    if dest == dst_track then
      return true
    end
  end
  return false
end

-- Main
local track, track_idx = find_talkback_track()

-- Create TALKBACK track if it doesn't exist
if not track then
  track, track_idx = create_talkback_track()
  if not track then
    reaper.SetExtState("reaperiem", "setup_talkback", "FAIL:create_track_failed", false)
    return
  end
end

-- Check if OIEM Receive already present
local already_present, fx_idx = has_oiem_receive(track)
if already_present then
  reaper.SetExtState("reaperiem", "setup_talkback", "SKIP:" .. track_idx .. ":" .. fx_idx, false)
  return
end

-- Insert OIEM Receive VST3
local new_fx = reaper.TrackFX_AddByName(track, "OIEM Receive", false, -1)
if new_fx < 0 then
  reaper.SetExtState("reaperiem", "setup_talkback", "FAIL:insert_fx_failed", false)
  return
end

-- Find all member inear tracks and create sends
local inear_tracks = find_member_inear_tracks()
if #inear_tracks == 0 then
  reaper.SetExtState("reaperiem", "setup_talkback", "FAIL:no_inear_tracks_found", false)
  return
end

local send_count = 0
for _, dest in ipairs(inear_tracks) do
  -- Skip if send already exists
  if not send_exists(track, dest.track) then
    local send_idx = reaper.CreateTrackSend(track, dest.track)
    if send_idx >= 0 then
      -- Set pre-fader post-FX (mode 3)
      reaper.SetTrackSendInfo_Value(track, 0, send_idx, "I_SENDMODE", 3)
      -- Unity gain (1.0)
      reaper.SetTrackSendInfo_Value(track, 0, send_idx, "D_VOL", 1.0)
      -- Center pan (0.0)
      reaper.SetTrackSendInfo_Value(track, 0, send_idx, "D_PAN", 0.0)
      send_count = send_count + 1
    end
  end
end

reaper.SetExtState("reaperiem", "setup_talkback", "OK:" .. track_idx .. ":" .. new_fx .. ":sends=" .. send_count, false)
