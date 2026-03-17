-- enum_reastream_params.lua
-- READ-ONLY: Enumerate all ReaStream VST parameters with names and values
-- Does NOT modify anything - safe to run
--
-- Result written to EXTSTATE key: reaperiem/reastream_params
-- Format: param_count=N|0:name=val|1:name=val|...

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

-- Find ReaStream FX on track
local function find_reastream_fx(track)
  local fx_count = reaper.TrackFX_GetCount(track)
  for i = 0, fx_count - 1 do
    local _, fx_name = reaper.TrackFX_GetFXName(track, i)
    if fx_name:lower():match("reastream") then
      return i, fx_name
    end
  end
  return -1, nil
end

-- Main
local track, track_idx = find_engineer_track()
if not track then
  reaper.SetExtState("reaperiem", "reastream_params", "ERROR:track_not_found", false)
  return
end

local fx_idx, fx_name = find_reastream_fx(track)
if fx_idx < 0 then
  reaper.SetExtState("reaperiem", "reastream_params", "ERROR:reastream_not_found", false)
  return
end

-- Check if FX is enabled
local enabled = reaper.TrackFX_GetEnabled(track, fx_idx)

-- Enumerate all parameters
local num_params = reaper.TrackFX_GetNumParams(track, fx_idx)
local parts = {}
parts[#parts + 1] = "fx_name=" .. (fx_name or "?")
parts[#parts + 1] = "fx_idx=" .. fx_idx
parts[#parts + 1] = "enabled=" .. (enabled and "yes" or "no")
parts[#parts + 1] = "param_count=" .. num_params

for p = 0, num_params - 1 do
  local _, pname = reaper.TrackFX_GetParamName(track, fx_idx, p)
  local val, minval, maxval = reaper.TrackFX_GetParam(track, fx_idx, p)
  -- Also get formatted value
  local _, formatted = reaper.TrackFX_GetFormattedParamValue(track, fx_idx, p)
  parts[#parts + 1] = p .. ":" .. (pname or "?") .. "=" .. val .. "[" .. (formatted or "?") .. "]" .. "(min=" .. minval .. ",max=" .. maxval .. ")"
end

reaper.SetExtState("reaperiem", "reastream_params", table.concat(parts, "|"), false)
