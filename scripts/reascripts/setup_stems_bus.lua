-- Setup Stems Bus Tracks
-- Creates per-member stems bus tracks and re-routes stem sends through them.
-- Called via HTTP API action ID: _RS_REAPERIEM_SETUP_STEMS_BUS
--
-- For each "{NAME} inear" track, creates "{NAME} stems" track at END of track list.
-- Stem channels (DRUMS, BASS, INST, CLICK, GUIDE, BGVS, OTHER) get re-routed
-- through the bus; mic/gtr/hand/tech channels keep their direct sends to inear.
--
-- This script is idempotent: if stems tracks already exist, it skips creation.
-- Result written to ExtState: reaperiem/stems_bus_result

local section = "reaperiem"

-- Categories that are "stems" (routed through the bus)
local stem_categories = {
    ["DRUMS"] = true,
    ["BASS"] = true,
    ["INST"] = true,
    ["CLICK"] = true,
    ["GUIDE"] = true,
    ["BGVS"] = true,
    ["OTHER"] = true,
}

-- Check if a track name is a stem input (first word is a stem category)
local function is_stem_input(name)
    local first_word = name:match("^(%S+)")
    return first_word and stem_categories[first_word] or false
end

-- Find track by name (returns MediaTrack or nil, and 1-based index)
local function find_track_by_name(target_name)
    local count = reaper.CountTracks(0)
    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)
        if name == target_name then
            return track, i + 1
        end
    end
    return nil, nil
end

-- Find all "inear" tracks (member output tracks)
local function find_inear_tracks()
    local result = {}
    local count = reaper.CountTracks(0)
    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)
        if name:match(" inear$") then
            local member_name = name:gsub(" inear$", "")
            table.insert(result, {
                track = track,
                index = i + 1,
                name = name,
                member_name = member_name,
                stems_name = member_name .. " stems",
            })
        end
    end
    return result
end

-- Find all input tracks (stem sources)
local function find_input_tracks()
    local result = {}
    local count = reaper.CountTracks(0)
    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, name = reaper.GetTrackName(track)
        if is_stem_input(name) then
            table.insert(result, {
                track = track,
                index = i + 1,
                name = name,
            })
        end
    end
    return result
end

-- Find send index from source_track to dest_track
local function find_send_to(source_track, dest_track)
    local num_sends = reaper.GetTrackNumSends(source_track, 0) -- 0 = sends
    for si = 0, num_sends - 1 do
        local dest = reaper.GetTrackSendInfo_Value(source_track, 0, si, "P_DESTTRACK")
        if dest == dest_track then
            return si
        end
    end
    return nil
end

-- Phase 1: Snapshot all stem send values for verification
local function snapshot_sends(input_tracks, inear_tracks)
    local snapshot = {}
    for _, input in ipairs(input_tracks) do
        for _, inear in ipairs(inear_tracks) do
            local si = find_send_to(input.track, inear.track)
            if si then
                local vol = reaper.GetTrackSendInfo_Value(input.track, 0, si, "D_VOL")
                local pan = reaper.GetTrackSendInfo_Value(input.track, 0, si, "D_PAN")
                local mute = reaper.GetTrackSendInfo_Value(input.track, 0, si, "B_MUTE")
                local mode = reaper.GetTrackSendInfo_Value(input.track, 0, si, "I_SENDMODE")
                local key = input.name .. "|" .. inear.member_name
                snapshot[key] = {
                    vol = vol,
                    pan = pan,
                    mute = mute,
                    mode = mode,
                    input_name = input.name,
                    member_name = inear.member_name,
                }
            end
        end
    end
    return snapshot
end

reaper.Undo_BeginBlock()
reaper.PreventUIRefresh(1)

local inear_tracks = find_inear_tracks()
local input_tracks = find_input_tracks()

if #inear_tracks == 0 then
    reaper.SetExtState(section, "stems_bus_result", "error: no inear tracks found", false)
    reaper.PreventUIRefresh(-1)
    reaper.Undo_EndBlock("Setup Stems Bus (aborted)", -1)
    return
end

-- Phase 1: Snapshot
local snapshot = snapshot_sends(input_tracks, inear_tracks)
local snapshot_count = 0
for _ in pairs(snapshot) do snapshot_count = snapshot_count + 1 end

-- Save snapshot to ExtState for debugging
local snap_str = ""
for key, s in pairs(snapshot) do
    snap_str = snap_str .. key .. "=" .. string.format("%.6f,%.6f,%d,%d", s.vol, s.pan, s.mute, s.mode) .. ";"
end
reaper.SetExtState(section, "stems_bus_snapshot", snap_str, false)

-- Phase 2: Create stems bus tracks (at END, idempotent)
local created = 0
local stems_tracks = {} -- member_name -> {track, index}

for _, inear in ipairs(inear_tracks) do
    local existing, existing_idx = find_track_by_name(inear.stems_name)
    if existing then
        stems_tracks[inear.member_name] = { track = existing, index = existing_idx }
    else
        -- Insert at end of track list
        local total = reaper.CountTracks(0)
        reaper.InsertTrackAtIndex(total, false)
        local new_track = reaper.GetTrack(0, total)
        reaper.GetSetMediaTrackInfo_String(new_track, "P_NAME", inear.stems_name, true)

        -- Create send from stems bus -> inear at 0 dB (unity gain), post-fader
        -- Post-fader so the stems group fader actually controls audio level
        local send_idx = reaper.CreateTrackSend(new_track, inear.track)
        if send_idx >= 0 then
            reaper.SetTrackSendInfo_Value(new_track, 0, send_idx, "D_VOL", 1.0) -- 0 dB
            reaper.SetTrackSendInfo_Value(new_track, 0, send_idx, "D_PAN", 0.0)
            reaper.SetTrackSendInfo_Value(new_track, 0, send_idx, "B_MUTE", 0)
            reaper.SetTrackSendInfo_Value(new_track, 0, send_idx, "I_SENDMODE", 0) -- post-fader (stems fader works)
        end

        stems_tracks[inear.member_name] = { track = new_track, index = total + 1 }
        created = created + 1
    end
end

-- Phase 3: Re-route stem sends from input -> inear to input -> stems bus
local rerouted = 0
local errors = {}

for _, input in ipairs(input_tracks) do
    for _, inear in ipairs(inear_tracks) do
        local stems = stems_tracks[inear.member_name]
        if not stems then goto continue_inner end

        -- Check if already routed to stems bus
        local si_to_stems = find_send_to(input.track, stems.track)
        if si_to_stems then
            -- Already routed, skip
            goto continue_inner
        end

        -- Find send to inear
        local si = find_send_to(input.track, inear.track)
        if not si then goto continue_inner end

        -- Save current values
        local vol = reaper.GetTrackSendInfo_Value(input.track, 0, si, "D_VOL")
        local pan = reaper.GetTrackSendInfo_Value(input.track, 0, si, "D_PAN")
        local mute = reaper.GetTrackSendInfo_Value(input.track, 0, si, "B_MUTE")
        local mode = reaper.GetTrackSendInfo_Value(input.track, 0, si, "I_SENDMODE")

        -- Change destination to stems bus by removing old send and creating new one
        reaper.RemoveTrackSend(input.track, 0, si)
        local new_si = reaper.CreateTrackSend(input.track, stems.track)
        if new_si >= 0 then
            reaper.SetTrackSendInfo_Value(input.track, 0, new_si, "D_VOL", vol)
            reaper.SetTrackSendInfo_Value(input.track, 0, new_si, "D_PAN", pan)
            reaper.SetTrackSendInfo_Value(input.track, 0, new_si, "B_MUTE", mute)
            reaper.SetTrackSendInfo_Value(input.track, 0, new_si, "I_SENDMODE", mode)
            rerouted = rerouted + 1
        else
            table.insert(errors, "Failed to create send: " .. input.name .. " -> " .. inear.stems_name)
        end

        ::continue_inner::
    end
end

-- Phase 4: Verify — re-read all values and compare against snapshot
local verify_ok = 0
local verify_fail = 0

for key, s in pairs(snapshot) do
    local input_track_obj = nil
    local stems_track_obj = nil

    -- Find the input track and stems bus
    for _, input in ipairs(input_tracks) do
        if input.name == s.input_name then
            input_track_obj = input.track
            break
        end
    end
    local stems = stems_tracks[s.member_name]
    if stems then stems_track_obj = stems.track end

    if input_track_obj and stems_track_obj then
        local si = find_send_to(input_track_obj, stems_track_obj)
        if si then
            local vol = reaper.GetTrackSendInfo_Value(input_track_obj, 0, si, "D_VOL")
            local pan = reaper.GetTrackSendInfo_Value(input_track_obj, 0, si, "D_PAN")
            local mute = reaper.GetTrackSendInfo_Value(input_track_obj, 0, si, "B_MUTE")

            if math.abs(vol - s.vol) < 0.0001 and math.abs(pan - s.pan) < 0.0001 and mute == s.mute then
                verify_ok = verify_ok + 1
            else
                verify_fail = verify_fail + 1
                table.insert(errors, string.format(
                    "Value mismatch: %s vol=%.6f/%.6f pan=%.6f/%.6f mute=%d/%d",
                    key, vol, s.vol, pan, s.pan, mute, s.mute
                ))
            end
        else
            verify_fail = verify_fail + 1
            table.insert(errors, "Send not found after reroute: " .. key)
        end
    end
end

reaper.PreventUIRefresh(-1)
reaper.TrackList_AdjustWindows(false)
reaper.UpdateArrange()
reaper.Undo_EndBlock("Setup Stems Bus Tracks", -1)

-- Write result
local result = string.format(
    "OK:created=%d,rerouted=%d,snapshot=%d,verified=%d,failed=%d",
    created, rerouted, snapshot_count, verify_ok, verify_fail
)
if #errors > 0 then
    result = result .. "|errors:" .. table.concat(errors, ";")
end
reaper.SetExtState(section, "stems_bus_result", result, false)
