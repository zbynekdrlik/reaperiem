-- tone_generator.lua
-- Inserts or removes a JS: Tone Generator on ENGINEER inear track
-- Controlled via EXTSTATE: reaperiem/tone_gen_action = "start" or "stop"
-- Results written to EXTSTATE: reaperiem/tone_gen_result

local action = reaper.GetExtState("reaperiem", "tone_gen_action")
if action == "" then
  reaper.SetExtState("reaperiem", "tone_gen_result", "ERROR:no_action", false)
  return
end

-- Find ENGINEER inear track
local function find_engineer_inear()
  for i = 0, reaper.CountTracks(0) - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:lower():find("engineer") and name:lower():find("inear") then
      return track, i + 1  -- 1-indexed track number
    end
  end
  return nil
end

local track, track_num = find_engineer_inear()
if not track then
  reaper.SetExtState("reaperiem", "tone_gen_result", "ERROR:engineer_inear_not_found", false)
  return
end

if action == "start" then
  -- Check if tone gen already exists
  local fx_count = reaper.TrackFX_GetCount(track)
  for i = 0, fx_count - 1 do
    local _, fx_name = reaper.TrackFX_GetFXName(track, i)
    if fx_name:lower():find("tone generator") then
      reaper.SetExtState("reaperiem", "tone_gen_result", "OK:already_present:fx_idx=" .. i, false)
      return
    end
  end

  -- Insert JS: Tone Generator at beginning of FX chain (before VBAN)
  local fx_idx = reaper.TrackFX_AddByName(track, "JS:Tone Generator", false, -1000)
  if fx_idx < 0 then
    -- Try alternative name
    fx_idx = reaper.TrackFX_AddByName(track, "Tone Generator", false, -1000)
  end

  if fx_idx >= 0 then
    -- Enable the FX
    reaper.TrackFX_SetEnabled(track, fx_idx, true)

    -- Configure: 440 Hz, -12 dB level
    -- JS: Tone Generator parameters: slider1=frequency, slider2=volume(dB)
    reaper.TrackFX_SetParam(track, fx_idx, 0, 440.0)  -- Frequency
    reaper.TrackFX_SetParam(track, fx_idx, 1, 0.25)    -- Volume (0.25 = -12dB normalized)

    reaper.SetExtState("reaperiem", "tone_gen_result",
      "OK:inserted:fx_idx=" .. fx_idx .. ":track=" .. track_num, false)
  else
    reaper.SetExtState("reaperiem", "tone_gen_result", "ERROR:insert_failed", false)
  end

elseif action == "stop" then
  -- Find and remove tone generator
  local fx_count = reaper.TrackFX_GetCount(track)
  local removed = false
  for i = fx_count - 1, 0, -1 do  -- Iterate backwards to handle index shifts
    local _, fx_name = reaper.TrackFX_GetFXName(track, i)
    if fx_name:lower():find("tone generator") then
      reaper.TrackFX_Delete(track, i)
      removed = true
    end
  end

  if removed then
    reaper.SetExtState("reaperiem", "tone_gen_result", "OK:removed", false)
  else
    reaper.SetExtState("reaperiem", "tone_gen_result", "OK:not_found", false)
  end
end

-- Clear the action
reaper.SetExtState("reaperiem", "tone_gen_action", "", false)
