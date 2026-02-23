-- Set Hardware Output for Track
-- Called via HTTP API with parameters passed through ExtState
--
-- ExtState keys (section: reaperiem):
--   hw_track_index: 1-based track index
--   hw_channel_l: Left output channel (1-based, will convert to 0-based)
--   hw_channel_r: Right output channel (1-based, will convert to 0-based)

local section = "reaperiem"

-- Read parameters from ExtState
local track_index = tonumber(reaper.GetExtState(section, "hw_track_index"))
local channel_l = tonumber(reaper.GetExtState(section, "hw_channel_l"))
local channel_r = tonumber(reaper.GetExtState(section, "hw_channel_r"))

-- Validate parameters
if not track_index or not channel_l or not channel_r then
    reaper.ShowConsoleMsg("Error: Missing parameters in ExtState\n")
    reaper.SetExtState(section, "hw_result", "error: missing parameters", false)
    return
end

-- Get the track (convert 1-based to 0-based for API)
local track = reaper.GetTrack(0, track_index - 1)
if not track then
    reaper.ShowConsoleMsg("Error: Track " .. track_index .. " not found\n")
    reaper.SetExtState(section, "hw_result", "error: track not found", false)
    return
end

local track_name = select(2, reaper.GetTrackName(track))

-- Convert 1-based channel numbers to 0-based
local ch_l_0based = channel_l - 1
local ch_r_0based = channel_r - 1

-- Check if hardware outputs already exist, remove them first
local num_hw_outs = reaper.GetTrackNumSends(track, 1)  -- 1 = hardware outputs
for i = num_hw_outs - 1, 0, -1 do
    reaper.RemoveTrackSend(track, 1, i)
end

-- Create new hardware output (pass nil/nothing as second param)
-- For stereo, we create one send that covers both channels
local hw_out_idx = reaper.CreateTrackSend(track, nil)

if hw_out_idx < 0 then
    reaper.ShowConsoleMsg("Error: Failed to create hardware output\n")
    reaper.SetExtState(section, "hw_result", "error: failed to create hw out", false)
    return
end

-- Set destination channel
-- I_DSTCHAN: destination channel index (0-based)
-- For stereo pair starting at channel_l: just use the left channel index
-- The stereo pair is automatic unless we set mono flag (&1024)
reaper.SetTrackSendInfo_Value(track, 1, hw_out_idx, "I_DSTCHAN", ch_l_0based)

-- Set volume to unity (1.0 = 0dB)
reaper.SetTrackSendInfo_Value(track, 1, hw_out_idx, "D_VOL", 1.0)

-- Set pan to center
reaper.SetTrackSendInfo_Value(track, 1, hw_out_idx, "D_PAN", 0.0)

-- Success message
local msg = string.format("Set %s hardware output to channels %d-%d",
    track_name, channel_l, channel_r)
reaper.ShowConsoleMsg(msg .. "\n")
reaper.SetExtState(section, "hw_result", "success: " .. msg, false)

-- Clear the input parameters
reaper.SetExtState(section, "hw_track_index", "", false)
reaper.SetExtState(section, "hw_channel_l", "", false)
reaper.SetExtState(section, "hw_channel_r", "", false)
