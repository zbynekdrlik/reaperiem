-- Meter Bridge for IEM Mixer
-- Continuously reads per-channel peak meters from all tracks and writes
-- to ExtState for the IEM Mixer's HTTP poller to read.
--
-- This provides TRUE stereo (L/R) instantaneous peak values, unlike the
-- TRACK HTTP response fields which only give combined peak-hold values.
--
-- Usage: Register in reaper-kb.ini, trigger once via HTTP API.
--        Runs continuously via defer() at REAPER's native frame rate.
--
-- ExtState output:
--   Section: REAPERIEM_METERS
--   Key:     peaks
--   Format:  "1:-37,-37;2:-911,-911;..."  (track_idx:L_db10,R_db10)
--
-- Values are dB*10 integers matching REAPER HTTP API convention.
-- Floor value: -1500 (= -150 dB = digital silence)

local SECTION = "REAPERIEM_METERS"
local KEY = "peaks"
local FLOOR = -1500
local RUNNING_KEY = "bridge_running"

-- Prevent duplicate instances
local already_running = reaper.GetExtState(SECTION, RUNNING_KEY)
if already_running == "1" then
  -- Check if it's actually still alive (stale flag from crash)
  -- We set a heartbeat timestamp; if it's older than 2s, assume dead
  local heartbeat = tonumber(reaper.GetExtState(SECTION, "heartbeat") or "0") or 0
  if reaper.time_precise() - heartbeat < 2.0 then
    -- Already running, skip silently
    return
  end
end

reaper.SetExtState(SECTION, RUNNING_KEY, "1", false)
-- Started silently (no console output to avoid UI interruption)

-- Convert linear amplitude to dB*10 integer
local function linear_to_db10(val)
  if val <= 0 then
    return FLOOR
  end
  local db = 20 * math.log(val, 10)
  local db10 = math.floor(db * 10 + 0.5)
  if db10 < FLOOR then
    return FLOOR
  end
  return db10
end

function main()
  -- Update heartbeat for duplicate detection
  reaper.SetExtState(SECTION, "heartbeat", tostring(reaper.time_precise()), false)

  local track_count = reaper.CountTracks(0)
  local parts = {}

  for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    if track then
      local track_idx = i + 1  -- 1-based (matches HTTP API convention)
      -- Track_GetPeakInfo returns instantaneous peak since last call (linear)
      -- Channel 0 = Left, Channel 1 = Right
      local peak_l = reaper.Track_GetPeakInfo(track, 0)
      local peak_r = reaper.Track_GetPeakInfo(track, 1)

      local l_db10 = linear_to_db10(peak_l)
      local r_db10 = linear_to_db10(peak_r)

      parts[#parts + 1] = track_idx .. ":" .. l_db10 .. "," .. r_db10
    end
  end

  reaper.SetExtState(SECTION, KEY, table.concat(parts, ";"), false)
  reaper.defer(main)
end

-- Cleanup on script exit
reaper.atexit(function()
  reaper.SetExtState(SECTION, RUNNING_KEY, "0", false)
  reaper.SetExtState(SECTION, KEY, "", false)
  -- Stopped silently
end)

main()
