-- check_vban.lua
-- Verify VBAN IEM VST3 presence and status on the ENGINEER inear track
-- Action ID: _RS_REAPERIEM_CHECK_VBAN
-- Usage: Trigger via HTTP API, then read EXTSTATE for result
--
-- Remote verification:
--   curl http://iem.lan:8080/_/_RS_REAPERIEM_CHECK_VBAN
--   curl http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/vban_status
--
-- Result format (EXTSTATE key: reaperiem/vban_status):
--   PRESENT:track_idx=N:fx_idx=N:enabled=yes
--   ABSENT:track_idx=N:fx_count=N
--   ERROR:reason

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

-- Main
local track, track_idx = find_engineer_track()
if not track then
  reaper.SetExtState("reaperiem", "vban_status", "ERROR:engineer_inear_track_not_found", false)
  return
end

-- Scan all FX for VBAN IEM
local fx_count = reaper.TrackFX_GetCount(track)
for i = 0, fx_count - 1 do
  local _, fx_name = reaper.TrackFX_GetFXName(track, i)
  if fx_name:lower():match("vban iem") then
    -- Found VBAN IEM — check if enabled
    local enabled = reaper.TrackFX_GetEnabled(track, i)
    local enabled_str = enabled and "yes" or "no"

    local status = "PRESENT:track_idx=" .. track_idx
      .. ":fx_idx=" .. i
      .. ":enabled=" .. enabled_str
      .. ":fx_name=" .. fx_name

    reaper.SetExtState("reaperiem", "vban_status", status, false)
    return
  end
end

-- VBAN IEM not found — also check if old ReaStream is still present
local has_reastream = false
for i = 0, fx_count - 1 do
  local _, fx_name = reaper.TrackFX_GetFXName(track, i)
  if fx_name:lower():match("reastream") then
    has_reastream = true
    break
  end
end

local status = "ABSENT:track_idx=" .. track_idx .. ":fx_count=" .. fx_count
if has_reastream then
  status = status .. ":warning=reastream_still_present"
end
reaper.SetExtState("reaperiem", "vban_status", status, false)
