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

-- Limiter activity tracking (#145)
-- Per-inear-track cumulative active milliseconds where GR < -1.0 dB.
-- Resets only when EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset is set to that
-- track index (the iem-mixer server writes this in response to the user
-- clicking Reset in the LimiterModal).
local LIMITER_SECTION = "REAPERIEM_LIMITER_ACTIVITY"
local LIMITER_TOTALS_KEY = "totals"
local LIMITER_RESET_KEY = "reset"
local LIMITER_GR_THRESHOLD_DB = -1.0  -- counts as "active" when slider5 < this
local LIMITER_FX_NAME_PATTERN = "MGA_JSLimiter"

-- Per-track active_ms accumulator: map<track_index_1based, integer_ms>
local limiter_active_ms = {}
-- Tick timestamp tracking (so we attribute exact wall delta, not fixed assumed dt)
local last_tick_time = nil

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
  -- Self-reload request (#145) — CI sets this after deploying new meter_bridge.lua
  -- so the running instance exits cleanly and the freshly-triggered instance
  -- picks up the new code. Without this, defer-based scripts keep executing
  -- the code they were started with even after the file on disk changes.
  local reload_request = reaper.GetExtState(SECTION, "reload_self")
  if reload_request == "1" then
    reaper.SetExtState(SECTION, RUNNING_KEY, "0", false)
    reaper.SetExtState(SECTION, "reload_self", "", false)
    return  -- No defer call — instance exits.
  end

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


  -- Limiter activity polling (#145).
  -- For every track that has our JS limiter, read slider5 (GR readout in dB,
  -- written by the JSFX from ext_gr_meter via sliderchange()), accumulate
  -- elapsed wall time into limiter_active_ms whenever slider5 < threshold.
  local now = reaper.time_precise()
  local dt_ms = 0
  if last_tick_time then
    dt_ms = math.floor((now - last_tick_time) * 1000.0 + 0.5)
    -- Clamp huge deltas (defer pause, REAPER backgrounded) so a 30 s pause
    -- doesn't show up as 30 s of limiter activity.
    if dt_ms > 250 then dt_ms = 0 end
  end
  last_tick_time = now

  -- Reset request handling — server writes track index here when user clicks Reset.
  local reset_request = reaper.GetExtState(LIMITER_SECTION, LIMITER_RESET_KEY)
  if reset_request ~= "" then
    local reset_idx = tonumber(reset_request)
    if reset_idx then
      limiter_active_ms[reset_idx] = 0
    end
    reaper.SetExtState(LIMITER_SECTION, LIMITER_RESET_KEY, "", false)
  end

  local lim_parts = {}
  for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    if track then
      local fx_count = reaper.TrackFX_GetCount(track)
      local fx_idx = -1
      for f = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, f)
        if fx_name and fx_name:find(LIMITER_FX_NAME_PATTERN, 1, true) then
          fx_idx = f
          break
        end
      end
      if fx_idx >= 0 then
        local track_idx = i + 1  -- 1-based to match meter convention
        local gr_db = reaper.TrackFX_GetParam(track, fx_idx, 4)  -- slider5
        if dt_ms > 0 and gr_db < LIMITER_GR_THRESHOLD_DB then
          limiter_active_ms[track_idx] = (limiter_active_ms[track_idx] or 0) + dt_ms
        end
        local total = limiter_active_ms[track_idx] or 0
        lim_parts[#lim_parts + 1] = track_idx .. ":" .. total
      end
    end
  end
  reaper.SetExtState(LIMITER_SECTION, LIMITER_TOTALS_KEY, table.concat(lim_parts, ";"), false)

  reaper.SetExtState(SECTION, KEY, table.concat(parts, ";"), false)

  -- Dynamic script registration via EXTSTATE (no REAPER restart needed)
  -- CI or curl sets "reaperiem/register_scripts" with pipe-delimited filenames
  -- e.g. "setup_vban.lua|check_vban.lua"
  local reg_request = reaper.GetExtState("reaperiem", "register_scripts")
  if reg_request ~= "" then
    local scripts_dir = reaper.GetResourcePath() .. "/Scripts/reaperiem/"
    local count = 0
    local ids = {}
    for filename in reg_request:gmatch("[^|]+") do
      local full_path = scripts_dir .. filename
      local cmd_id = reaper.AddRemoveReaScript(true, 0, full_path, true)
      if cmd_id ~= 0 then
        count = count + 1
        -- Store numeric action ID so CI can trigger the script
        -- Key: "action_<basename>" e.g. "action_setup_vban"
        local base = filename:gsub("%.lua$", "")
        reaper.SetExtState("reaperiem", "action_" .. base, tostring(cmd_id), false)
        ids[#ids + 1] = base .. "=" .. cmd_id
      end
    end
    reaper.SetExtState("reaperiem", "register_result", "OK:" .. count .. ":" .. table.concat(ids, "|"), false)
    reaper.SetExtState("reaperiem", "register_scripts", "", false)
  end

  reaper.defer(main)
end

-- Cleanup on script exit
reaper.atexit(function()
  reaper.SetExtState(SECTION, RUNNING_KEY, "0", false)
  reaper.SetExtState(SECTION, KEY, "", false)
  -- Stopped silently
end)

main()
