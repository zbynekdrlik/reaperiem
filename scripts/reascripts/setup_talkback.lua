-- setup_talkback.lua
-- Insert OIEM Receive VST3 on ENGINEER mic track for talkback (#123)
-- Action ID: _RS_REAPERIEM_SETUP_TALKBACK
--
-- The OIEM Receive VST is an EFFECT that mixes network talkback audio
-- with the existing Dante mic signal on the ENGINEER mic track.
-- Band members already have sends from ENGINEER mic — no extra track needed.
--
-- Also cleans up any leftover TALKBACK track from earlier implementation.
--
-- Result codes:
--   OK:<track_idx>:<fx_idx>   — inserted successfully
--   SKIP:<track_idx>:<fx_idx> — already present
--   FAIL:<reason>             — error

-- Find ENGINEER mic track
local function find_engineer_mic()
  local count = reaper.CountTracks(0)
  for i = 0, count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:lower():match("engineer") and name:lower():match("mic") then
      return track, i
    end
  end
  return nil, -1
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

-- Remove leftover TALKBACK track if it exists (cleanup from v1 implementation)
local function cleanup_talkback_track()
  local count = reaper.CountTracks(0)
  for i = count - 1, 0, -1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:upper() == "TALKBACK" then
      reaper.DeleteTrack(track)
      return true
    end
  end
  return false
end

-- Main
local track, track_idx = find_engineer_mic()
if not track then
  reaper.SetExtState("reaperiem", "setup_talkback", "FAIL:engineer_mic_not_found", false)
  return
end

-- Ensure ENGINEER mic is record-armed with monitoring (needed for FX processing)
reaper.SetMediaTrackInfo_Value(track, "I_RECARM", 1)
reaper.SetMediaTrackInfo_Value(track, "I_RECMON", 1)

-- Check if OIEM Receive already present
local already_present, fx_idx = has_oiem_receive(track)
if already_present then
  -- Clean up old TALKBACK track if it exists
  cleanup_talkback_track()
  reaper.SetExtState("reaperiem", "setup_talkback", "SKIP:" .. track_idx .. ":" .. fx_idx, false)
  return
end

-- Insert OIEM Receive VST3 on ENGINEER mic track
local new_fx = reaper.TrackFX_AddByName(track, "OIEM Receive", false, -1)
if new_fx < 0 then
  reaper.SetExtState("reaperiem", "setup_talkback", "FAIL:insert_fx_failed", false)
  return
end

-- Clean up old TALKBACK track if it exists
cleanup_talkback_track()

reaper.SetExtState("reaperiem", "setup_talkback", "OK:" .. track_idx .. ":" .. new_fx, false)
