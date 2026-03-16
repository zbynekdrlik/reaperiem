-- setup_reastream.lua
-- Insert ReaStream VST on the ENGINEER inear track for audio streaming
-- Action ID: _RS_REAPERIEM_SETUP_REASTREAM
-- Usage: One-time setup, idempotent (skips if ReaStream already present)
--
-- ReaStream sends float32 PCM via UDP to localhost:4711
-- The IEM Mixer Tauri app captures these packets and streams to the engineer's browser
--
-- Result is written to EXTSTATE for remote verification:
--   reaper.GetExtState("reaperiem", "setup_reastream")
--   OK:<track_idx>:<fx_idx>  — success
--   SKIP:<track_idx>:<fx_idx> — already present
--   FAIL:<reason>            — error

-- Find engineer inear track
local function find_engineer_track()
  local track_count = reaper.CountTracks(0)
  for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    -- Match "ENGINEER inear" (case-insensitive)
    if name:lower():match("engineer") and name:lower():match("inear") then
      return track, i
    end
  end
  return nil, -1
end

-- Check if ReaStream is already present on the track
local function has_reastream(track)
  local fx_count = reaper.TrackFX_GetCount(track)
  for i = 0, fx_count - 1 do
    local _, fx_name = reaper.TrackFX_GetFXName(track, i)
    if fx_name:lower():match("reastream") then
      return true, i
    end
  end
  return false, -1
end

-- Main
local track, track_idx = find_engineer_track()
if not track then
  reaper.SetExtState("reaperiem", "setup_reastream", "FAIL:engineer_inear_track_not_found", false)
  return
end

local already_present, fx_idx = has_reastream(track)
if already_present then
  reaper.SetExtState("reaperiem", "setup_reastream", "SKIP:" .. track_idx .. ":" .. fx_idx, false)
  return
end

-- Insert ReaStream VST
-- ReaStream is a VST2 plugin bundled with REAPER
local new_fx = reaper.TrackFX_AddByName(track, "ReaStream (Cockos)", false, -1)
if new_fx < 0 then
  reaper.SetExtState("reaperiem", "setup_reastream", "FAIL:insert_failed", false)
  return
end

-- Configure ReaStream:
-- Parameter 0: Mode (0.0 = Send, 1.0 = Receive)
-- ReaStream defaults to localhost:4711 when in send mode
-- IP/port are configured via the VST GUI (not scriptable parameters)
reaper.TrackFX_SetParam(track, new_fx, 0, 0.0)

local _, track_name = reaper.GetTrackName(track)
reaper.SetExtState("reaperiem", "setup_reastream", "OK:" .. track_idx .. ":" .. new_fx, false)
