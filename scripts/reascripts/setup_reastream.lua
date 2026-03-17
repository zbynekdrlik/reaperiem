-- setup_reastream.lua
-- Insert ReaStream VST on the ENGINEER inear track for audio streaming
-- Action ID: _RS_REAPERIEM_SETUP_REASTREAM
-- Usage: One-time setup, idempotent (skips if ReaStream already present)
--
-- ReaStream sends float32 PCM via UDP to the IEM Mixer app.
-- IMPORTANT: ReaStream IP/port are NOT accessible via ReaScript API.
-- After this script inserts ReaStream, you MUST configure in the GUI:
--   - Mode: Send
--   - IP: 127.0.0.1
--   - Port: 58710 (default — no change needed, app shares port via SO_REUSEADDR)
--
-- Result is written to EXTSTATE for remote verification:
--   reaper.GetExtState("reaperiem", "setup_reastream")
--   OK:<track_idx>:<fx_idx>  — inserted successfully (configure GUI!)
--   SKIP:<track_idx>:<fx_idx> — already present
--   FAIL:<reason>            — error

-- Find engineer inear track
local function find_engineer_track()
  local track_count = reaper.CountTracks(0)
  for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
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

-- Insert ReaStream VST (bundled with REAPER)
local new_fx = reaper.TrackFX_AddByName(track, "ReaStream (Cockos)", false, -1)
if new_fx < 0 then
  reaper.SetExtState("reaperiem", "setup_reastream", "FAIL:insert_failed", false)
  return
end

-- NOTE: ReaStream mode/IP/port are NOT exposed as VST parameters.
-- Only 4 params exist: resv, Bypass, Wet, Delta.
-- The user MUST configure Send mode, IP 127.0.0.1, port 58710 in the GUI.
reaper.SetExtState("reaperiem", "setup_reastream", "OK:" .. track_idx .. ":" .. new_fx, false)
