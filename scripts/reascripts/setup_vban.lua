-- setup_vban.lua
-- Insert VBAN IEM VST3 on the ENGINEER inear track for audio streaming
-- Action ID: _RS_REAPERIEM_SETUP_VBAN
-- Usage: One-time setup, idempotent (skips if VBAN IEM already present)
--
-- VBAN IEM VST3 sends INT16 PCM via UDP to 127.0.0.1:7980 (hardcoded in plugin).
-- The plugin auto-activates on load — no manual GUI configuration needed.
-- If ReaStream was previously used, this script removes it first.
--
-- Result is written to EXTSTATE for remote verification:
--   reaper.GetExtState("reaperiem", "setup_vban")
--   OK:<track_idx>:<fx_idx>  — inserted successfully
--   SKIP:<track_idx>:<fx_idx> — already present
--   REPLACED:<track_idx>:<fx_idx> — removed ReaStream, inserted VBAN
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

-- Check if VBAN IEM is already present on the track
local function has_vban_iem(track)
  local fx_count = reaper.TrackFX_GetCount(track)
  for i = 0, fx_count - 1 do
    local _, fx_name = reaper.TrackFX_GetFXName(track, i)
    if fx_name:lower():match("vban iem") then
      return true, i
    end
  end
  return false, -1
end

-- Check if ReaStream is present and remove it
local function remove_reastream(track)
  local fx_count = reaper.TrackFX_GetCount(track)
  for i = 0, fx_count - 1 do
    local _, fx_name = reaper.TrackFX_GetFXName(track, i)
    if fx_name:lower():match("reastream") then
      reaper.TrackFX_Delete(track, i)
      return true
    end
  end
  return false
end

-- Main
local track, track_idx = find_engineer_track()
if not track then
  reaper.SetExtState("reaperiem", "setup_vban", "FAIL:engineer_inear_track_not_found", false)
  return
end

-- Check if VBAN IEM already present
local already_present, fx_idx = has_vban_iem(track)
if already_present then
  reaper.SetExtState("reaperiem", "setup_vban", "SKIP:" .. track_idx .. ":" .. fx_idx, false)
  return
end

-- Remove ReaStream if present (migration path)
local had_reastream = remove_reastream(track)

-- Insert VBAN IEM VST3
local new_fx = reaper.TrackFX_AddByName(track, "VBAN IEM", false, -1)
if new_fx < 0 then
  reaper.SetExtState("reaperiem", "setup_vban", "FAIL:insert_failed", false)
  return
end

-- VBAN IEM auto-activates with hardcoded settings (127.0.0.1:7980, stream "engineer")
-- No manual GUI configuration needed (unlike ReaStream which required GUI setup)
if had_reastream then
  reaper.SetExtState("reaperiem", "setup_vban", "REPLACED:" .. track_idx .. ":" .. new_fx, false)
else
  reaper.SetExtState("reaperiem", "setup_vban", "OK:" .. track_idx .. ":" .. new_fx, false)
end
