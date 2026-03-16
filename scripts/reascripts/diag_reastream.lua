-- diag_reastream.lua
-- Diagnostic script: reads ReaStream VST configuration from track state chunk
-- Reports IP address, port, mode, and identifier to EXTSTATE
--
-- Action ID: _RS_REAPERIEM_DIAG_REASTREAM
--
-- Result is written to EXTSTATE key: reaperiem/reastream_diag
-- Format: key=value pairs separated by |
--   track_idx=N|fx_idx=N|mode=send|enabled=yes|ip=127.0.0.1|identifier=engineer
--   ERROR:reason

-- Minimal base64 decoder for reading VST chunk data
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

-- Extract IP-like strings from binary data
local function find_ip_strings(data)
  local ips = {}
  -- Look for dotted-decimal IP patterns in the binary data
  for ip in data:gmatch("(%d+%.%d+%.%d+%.%d+)") do
    ips[#ips + 1] = ip
  end
  return ips
end

-- Extract null-terminated strings from binary data
local function find_ascii_strings(data, min_len)
  min_len = min_len or 3
  local strings = {}
  local current = {}
  for i = 1, #data do
    local byte = data:byte(i)
    if byte >= 32 and byte <= 126 then
      current[#current + 1] = string.char(byte)
    else
      if #current >= min_len then
        strings[#strings + 1] = table.concat(current)
      end
      current = {}
    end
  end
  if #current >= min_len then
    strings[#strings + 1] = table.concat(current)
  end
  return strings
end

-- Main
local track, track_idx = find_engineer_track()
if not track then
  reaper.SetExtState("reaperiem", "reastream_diag", "ERROR:engineer_inear_track_not_found", false)
  return
end

local fx_idx, fx_name = find_reastream_fx(track)
if fx_idx < 0 then
  reaper.SetExtState("reaperiem", "reastream_diag", "ERROR:reastream_not_found", false)
  return
end

-- Basic info
local mode_val = reaper.TrackFX_GetParam(track, fx_idx, 0)
local mode_str = mode_val > 0.5 and "send" or "receive"
local enabled = reaper.TrackFX_GetEnabled(track, fx_idx)

-- Read track state chunk to find ReaStream configuration
local retval, chunk = reaper.GetTrackStateChunk(track, "", false)
local ip_found = "unknown"
local identifier_found = "unknown"
local port_found = "unknown"

if retval then
  -- Find the ReaStream VST section in the chunk
  -- Look for the VST block containing "reastream"
  local vst_start = chunk:lower():find("reastream")
  if vst_start then
    -- Extract base64 data lines after the VST declaration
    -- VST block format: <VST "name" dll ...  \n  base64lines \n >
    local block_start = chunk:sub(1, vst_start):find("<VST[^\n]*$")
    if not block_start then
      -- Try finding the < before the reastream match
      local search_region = chunk:sub(math.max(1, vst_start - 200), vst_start)
      local last_lt = 0
      for pos in search_region:gmatch("()<VST") do
        last_lt = pos
      end
      if last_lt > 0 then
        block_start = math.max(1, vst_start - 200) + last_lt - 1
      end
    end

    if block_start then
      -- Find the closing > for this VST block
      local block_end = chunk:find("\n>", vst_start)
      if block_end then
        local vst_block = chunk:sub(block_start, block_end + 1)

        -- Extract base64 data (lines that look like base64 — alphanumeric+/= only)
        local b64_lines = {}
        local line_num = 0
        for line in vst_block:gmatch("[^\n]+") do
          line_num = line_num + 1
          -- Skip first line (VST declaration) and lines starting with known keywords
          if line_num > 1 and not line:match("^%s*<") and not line:match("^%s*>")
             and not line:match("^%s*FLOAT") and not line:match("^%s*WAK")
             and not line:match("^%s*BYPASS") and not line:match("^%s*PRESETNAME") then
            local cleaned = line:match("^%s*(.-)%s*$")
            if cleaned and #cleaned > 0 then
              b64_lines[#b64_lines + 1] = cleaned
            end
          end
        end

        if #b64_lines > 0 then
          local b64_data = table.concat(b64_lines)
          local ok, binary = pcall(b64decode, b64_data)
          if ok and binary then
            -- Search for IP addresses in decoded binary
            local ips = find_ip_strings(binary)
            if #ips > 0 then
              ip_found = table.concat(ips, ",")
            end

            -- Search for ASCII strings (identifier, etc)
            local strings = find_ascii_strings(binary, 3)
            if #strings > 0 then
              identifier_found = table.concat(strings, ",")
            end
          end
        end
      end
    end
  end
end

local result = "track_idx=" .. track_idx
  .. "|fx_idx=" .. fx_idx
  .. "|fx_name=" .. (fx_name or "unknown")
  .. "|mode=" .. mode_str
  .. "|enabled=" .. (enabled and "yes" or "no")
  .. "|ip=" .. ip_found
  .. "|identifier=" .. identifier_found

reaper.SetExtState("reaperiem", "reastream_diag", result, false)
