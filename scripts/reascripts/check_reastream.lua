-- check_reastream.lua
-- Verify ReaStream VST presence and status on the ENGINEER inear track
-- Action ID: _RS_REAPERIEM_CHECK_REASTREAM
-- Usage: Trigger via HTTP API, then read EXTSTATE for result
--
-- Remote verification:
--   curl http://iem.lan:8080/_/_RS_REAPERIEM_CHECK_REASTREAM
--   curl http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/reastream_status
--
-- Result format (EXTSTATE key: reaperiem/reastream_status):
--   PRESENT:track_idx=N:fx_idx=N:mode=send:enabled=yes
--   ABSENT:engineer_track_found:fx_count=N
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
  reaper.SetExtState("reaperiem", "reastream_status", "ERROR:engineer_inear_track_not_found", false)
  return
end

-- Scan all FX for ReaStream
local fx_count = reaper.TrackFX_GetCount(track)
for i = 0, fx_count - 1 do
  local _, fx_name = reaper.TrackFX_GetFXName(track, i)
  if fx_name:lower():match("reastream") then
    -- Found ReaStream — check mode parameter
    local mode_val = reaper.TrackFX_GetParam(track, i, 0)
    local mode_str = "unknown"
    if mode_val < 0.5 then
      mode_str = "send"
    else
      mode_str = "receive"
    end

    -- Check if FX is enabled
    local enabled = reaper.TrackFX_GetEnabled(track, i)
    local enabled_str = enabled and "yes" or "no"

    local status = "PRESENT:track_idx=" .. track_idx
      .. ":fx_idx=" .. i
      .. ":mode=" .. mode_str
      .. ":enabled=" .. enabled_str
      .. ":fx_name=" .. fx_name

    reaper.SetExtState("reaperiem", "reastream_status", status, false)
    return
  end
end

-- ReaStream not found
local status = "ABSENT:track_idx=" .. track_idx .. ":fx_count=" .. fx_count
reaper.SetExtState("reaperiem", "reastream_status", status, false)
