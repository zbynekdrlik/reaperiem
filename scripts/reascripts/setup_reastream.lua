-- setup_reastream.lua
-- Insert ReaStream VST on the ENGINEER inear track for audio streaming
-- Action ID: _RS_REAPERIEM_SETUP_REASTREAM
-- Usage: One-time setup, idempotent (skips if ReaStream already present)
--
-- ReaStream sends float32 PCM via UDP to localhost:4711
-- The IEM Mixer Tauri app captures these packets and streams to the engineer's browser
--
-- After inserting ReaStream, this script attempts to configure the send IP
-- to 127.0.0.1 by modifying the VST state chunk. This ensures packets go
-- to localhost rather than broadcast (255.255.255.255) which may be blocked
-- by Windows Firewall.
--
-- Result is written to EXTSTATE for remote verification:
--   reaper.GetExtState("reaperiem", "setup_reastream")
--   OK:<track_idx>:<fx_idx>:<ip_status>  — success
--   SKIP:<track_idx>:<fx_idx>            — already present
--   FAIL:<reason>                         — error

-- Base64 decode/encode for VST chunk manipulation
local b64chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
local b64lookup = {}
for i = 1, #b64chars do
  b64lookup[b64chars:sub(i, i)] = i - 1
end

local function b64decode(str)
  str = str:gsub("[^A-Za-z0-9+/=]", "")
  local result = {}
  local i = 1
  while i <= #str do
    local a = b64lookup[str:sub(i, i)] or 0
    local b = b64lookup[str:sub(i + 1, i + 1)] or 0
    local c = b64lookup[str:sub(i + 2, i + 2)] or 0
    local d = b64lookup[str:sub(i + 3, i + 3)] or 0
    local n = a * 262144 + b * 4096 + c * 64 + d
    result[#result + 1] = string.char(math.floor(n / 65536) % 256)
    if str:sub(i + 2, i + 2) ~= "=" then
      result[#result + 1] = string.char(math.floor(n / 256) % 256)
    end
    if str:sub(i + 3, i + 3) ~= "=" then
      result[#result + 1] = string.char(n % 256)
    end
    i = i + 4
  end
  return table.concat(result)
end

local function b64encode(str)
  local result = {}
  local i = 1
  while i <= #str do
    local a = str:byte(i)
    local b = i + 1 <= #str and str:byte(i + 1) or 0
    local c = i + 2 <= #str and str:byte(i + 2) or 0
    local n = a * 65536 + b * 256 + c
    result[#result + 1] = b64chars:sub(math.floor(n / 262144) % 64 + 1, math.floor(n / 262144) % 64 + 1)
    result[#result + 1] = b64chars:sub(math.floor(n / 4096) % 64 + 1, math.floor(n / 4096) % 64 + 1)
    if i + 1 <= #str then
      result[#result + 1] = b64chars:sub(math.floor(n / 64) % 64 + 1, math.floor(n / 64) % 64 + 1)
    else
      result[#result + 1] = "="
    end
    if i + 2 <= #str then
      result[#result + 1] = b64chars:sub(n % 64 + 1, n % 64 + 1)
    else
      result[#result + 1] = "="
    end
    i = i + 3
  end
  return table.concat(result)
end

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

-- Attempt to configure ReaStream IP to 127.0.0.1 via track state chunk manipulation
-- Returns status string: "ip_set", "ip_already_ok", "ip_unchanged", "chunk_error"
local function configure_reastream_ip(track, fx_idx)
  local retval, chunk = reaper.GetTrackStateChunk(track, "", false)
  if not retval then
    return "chunk_read_error"
  end

  -- Find the ReaStream VST section in the chunk
  local lower_chunk = chunk:lower()
  local rs_pos = lower_chunk:find("reastream")
  if not rs_pos then
    return "reastream_not_in_chunk"
  end

  -- Find the <VST line containing ReaStream
  -- Walk backward from rs_pos to find the start of the VST block
  local block_start = nil
  local search_start = math.max(1, rs_pos - 300)
  local before = chunk:sub(search_start, rs_pos + 50)
  -- Find the last <VST before the reastream match
  local last_vst_pos = nil
  local pos = 1
  while true do
    local found = before:find("<VST", pos)
    if not found then break end
    last_vst_pos = found
    pos = found + 1
  end
  if last_vst_pos then
    block_start = search_start + last_vst_pos - 1
  end
  if not block_start then
    return "vst_block_not_found"
  end

  -- Find the end of the VST block (line with just >)
  local block_end = chunk:find("\n>", rs_pos)
  if not block_end then
    return "vst_block_end_not_found"
  end

  local vst_block = chunk:sub(block_start, block_end + 1)

  -- Extract base64 data lines (skip VST header line and metadata lines)
  local b64_lines = {}
  local b64_line_positions = {} -- track positions for replacement
  local line_num = 0
  for line in vst_block:gmatch("[^\n]+") do
    line_num = line_num + 1
    if line_num > 1
       and not line:match("^%s*<") and not line:match("^%s*>")
       and not line:match("^%s*FLOAT") and not line:match("^%s*WAK")
       and not line:match("^%s*BYPASS") and not line:match("^%s*PRESETNAME") then
      local cleaned = line:match("^%s*(.-)%s*$")
      if cleaned and #cleaned > 0 and cleaned:match("^[A-Za-z0-9+/=]+$") then
        b64_lines[#b64_lines + 1] = cleaned
      end
    end
  end

  if #b64_lines == 0 then
    return "no_base64_data"
  end

  local b64_data = table.concat(b64_lines)
  local ok, binary = pcall(b64decode, b64_data)
  if not ok or not binary then
    return "base64_decode_error"
  end

  -- Look for broadcast IP (255.255.255.255) in the binary data
  local broadcast_ip = "255.255.255.255"
  local localhost_ip = "127.0.0.1"
  local broadcast_pos = binary:find(broadcast_ip, 1, true)

  if not broadcast_pos then
    -- Check if it's already localhost
    local localhost_pos = binary:find(localhost_ip, 1, true)
    if localhost_pos then
      return "ip_already_ok"
    end
    -- IP not found as either broadcast or localhost — report what we see
    local ips = {}
    for ip in binary:gmatch("(%d+%.%d+%.%d+%.%d+)") do
      ips[#ips + 1] = ip
    end
    if #ips > 0 then
      return "ip_unknown:" .. table.concat(ips, ",")
    end
    return "ip_not_found_in_chunk"
  end

  -- Replace broadcast IP with localhost
  -- IMPORTANT: Both strings must be the same length to avoid corrupting the binary format
  -- "255.255.255.255" = 15 chars, "127.0.0.1" = 9 chars
  -- Pad with null bytes to maintain the same length
  local padded_localhost = localhost_ip .. string.rep("\0", #broadcast_ip - #localhost_ip)
  local new_binary = binary:sub(1, broadcast_pos - 1) .. padded_localhost .. binary:sub(broadcast_pos + #broadcast_ip)

  -- Re-encode to base64
  local new_b64 = b64encode(new_binary)

  -- Format base64 into 128-char lines (REAPER's convention)
  local new_b64_lines = {}
  for i = 1, #new_b64, 128 do
    new_b64_lines[#new_b64_lines + 1] = "    " .. new_b64:sub(i, math.min(i + 127, #new_b64))
  end
  local new_b64_block = table.concat(new_b64_lines, "\n")

  -- Replace the old base64 data in the VST block
  -- Build the new VST block: header line + new base64 + closing >
  local new_vst_lines = {}
  line_num = 0
  local in_b64 = false
  local b64_replaced = false
  for line in vst_block:gmatch("[^\n]+") do
    line_num = line_num + 1
    if line_num == 1 then
      -- VST header line — keep as-is
      new_vst_lines[#new_vst_lines + 1] = line
    elseif line:match("^%s*<") or line:match("^%s*>")
       or line:match("^%s*FLOAT") or line:match("^%s*WAK")
       or line:match("^%s*BYPASS") or line:match("^%s*PRESETNAME") then
      -- Metadata line — keep as-is
      if in_b64 and not b64_replaced then
        -- Insert new base64 data before this metadata line
        new_vst_lines[#new_vst_lines + 1] = new_b64_block
        b64_replaced = true
        in_b64 = false
      end
      new_vst_lines[#new_vst_lines + 1] = line
    else
      local cleaned = line:match("^%s*(.-)%s*$")
      if cleaned and #cleaned > 0 and cleaned:match("^[A-Za-z0-9+/=]+$") then
        -- Base64 line — skip (will be replaced)
        in_b64 = true
      else
        if in_b64 and not b64_replaced then
          new_vst_lines[#new_vst_lines + 1] = new_b64_block
          b64_replaced = true
          in_b64 = false
        end
        new_vst_lines[#new_vst_lines + 1] = line
      end
    end
  end
  if in_b64 and not b64_replaced then
    new_vst_lines[#new_vst_lines + 1] = new_b64_block
  end

  local new_vst_block = table.concat(new_vst_lines, "\n")

  -- Replace the old VST block in the track chunk
  local new_chunk = chunk:sub(1, block_start - 1) .. new_vst_block .. chunk:sub(block_end + 2)

  -- Apply the modified chunk
  local set_ok = reaper.SetTrackStateChunk(track, new_chunk, false)
  if not set_ok then
    return "chunk_write_error"
  end

  return "ip_set"
end

-- Main
local track, track_idx = find_engineer_track()
if not track then
  reaper.SetExtState("reaperiem", "setup_reastream", "FAIL:engineer_inear_track_not_found", false)
  return
end

local already_present, fx_idx = has_reastream(track)
if already_present then
  -- Even if already present, try to configure IP
  local ip_status = configure_reastream_ip(track, fx_idx)
  reaper.SetExtState("reaperiem", "setup_reastream",
    "SKIP:" .. track_idx .. ":" .. fx_idx .. ":" .. ip_status, false)
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
-- Parameter 0: Mode (0.0 = Receive, 1.0 = Send)
reaper.TrackFX_SetParam(track, new_fx, 0, 1.0)

-- Attempt to configure IP to 127.0.0.1 via chunk manipulation
-- This changes the send target from broadcast (255.255.255.255) to localhost
local ip_status = configure_reastream_ip(track, new_fx)

reaper.SetExtState("reaperiem", "setup_reastream",
  "OK:" .. track_idx .. ":" .. new_fx .. ":" .. ip_status, false)
