-- setup_reastream.lua
-- Insert ReaStream VST on the ENGINEER inear track for audio streaming
-- Action ID: _RS_REAPERIEM_SETUP_REASTREAM
-- Usage: One-time setup, idempotent (skips if ReaStream already present)
--
-- ReaStream sends float32 PCM via UDP to localhost:4711
-- The IEM Mixer Tauri app captures these packets and streams to the engineer's browser

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
  reaper.ShowConsoleMsg("setup_reastream: ENGINEER inear track not found!\n")
  return
end

local already_present, fx_idx = has_reastream(track)
if already_present then
  reaper.ShowConsoleMsg("setup_reastream: ReaStream already present on ENGINEER inear (FX #" .. fx_idx .. "), skipping.\n")
  return
end

-- Insert ReaStream VST
-- ReaStream is a VST2 plugin bundled with REAPER
local new_fx = reaper.TrackFX_AddByName(track, "ReaStream (Cockos)", false, -1)
if new_fx < 0 then
  reaper.ShowConsoleMsg("setup_reastream: Failed to insert ReaStream VST! Is it installed?\n")
  return
end

-- Configure ReaStream:
-- Parameter 0: Mode (0 = Send, 1 = Receive)
-- Parameter 1: IP address (string, not directly settable via param)
-- Parameter 2: Identifier
-- We set to Send mode, the identifier and IP are configured via the VST GUI defaults
-- ReaStream defaults to localhost:4711 for send mode

-- Set to Send mode (parameter index 0, value 0.0 = send)
reaper.TrackFX_SetParam(track, new_fx, 0, 0.0)

-- Set the identifier to "default" via named config
-- (ReaStream uses "default" as its channel identifier)
reaper.TrackFX_SetNamedConfigParm(track, new_fx, "identifier", "default")

local _, track_name = reaper.GetTrackName(track)
reaper.ShowConsoleMsg("setup_reastream: Inserted ReaStream (send mode) on track '" .. track_name .. "' (index " .. track_idx .. ")\n")
reaper.ShowConsoleMsg("setup_reastream: Streaming to localhost:4711 — open engineer mixer and click Listen\n")
