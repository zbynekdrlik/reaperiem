-- setup_mix_sends.lua
-- Creates cross-member inear sends for elevated members.
-- Triggered via: SET/EXTSTATE/reaperiem/elevated_members/<pipe-delimited member names>
-- Then: _RS_REAPERIEM_SETUP_MIX_SENDS
--
-- For each elevated member, creates a send FROM each other member's inear track
-- TO the elevated member's inear track. Sends are muted by default (volume 0dB, muted).
-- Idempotent: skips sends that already exist.

local function log(msg)
    reaper.ShowConsoleMsg(msg .. "\n")
end

-- Read elevated member names from EXTSTATE
local elevated_str = ({reaper.GetExtState("reaperiem", "elevated_members")})[2] or ""
if elevated_str == "" then
    reaper.SetExtState("reaperiem", "mix_sends_result", "ERROR:no_elevated_members", false)
    return
end

-- Parse pipe-delimited member names (e.g., "PETRONELA|MAREK")
local elevated_names = {}
for name in elevated_str:gmatch("[^|]+") do
    elevated_names[name:upper()] = true
end

-- Discover all inear tracks
local inear_tracks = {}  -- name -> MediaTrack
local track_count = reaper.CountTracks(0)
for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:match(" inear$") or name:match(" INEAR$") then
        local member_name = name:gsub(" [iI][nN][eE][aA][rR]$", "")
        inear_tracks[member_name:upper()] = track
    end
end

-- Check if a send from src to dst already exists
local function send_exists(src_track, dst_track)
    local num_sends = reaper.GetTrackNumSends(src_track, 0) -- category 0 = sends
    for i = 0, num_sends - 1 do
        local dest = reaper.GetTrackSendInfo_Value(src_track, 0, i, "P_DESTTRACK")
        if dest == dst_track then
            return true
        end
    end
    return false
end

reaper.Undo_BeginBlock()
reaper.PreventUIRefresh(1)

local created = 0
local skipped = 0
local errors = 0

for elevated_name, _ in pairs(elevated_names) do
    local elevated_track = inear_tracks[elevated_name]
    if not elevated_track then
        log("WARNING: No inear track found for elevated member: " .. elevated_name)
        errors = errors + 1
        goto continue_elevated
    end

    -- Create sends from each OTHER member's inear to this elevated member's inear
    for other_name, other_track in pairs(inear_tracks) do
        if other_name ~= elevated_name and other_name ~= "ENGINEER" then
            if send_exists(other_track, elevated_track) then
                skipped = skipped + 1
            else
                local send_idx = reaper.CreateTrackSend(other_track, elevated_track)
                if send_idx >= 0 then
                    -- Set volume to 0dB, muted by default
                    reaper.SetTrackSendInfo_Value(other_track, 0, send_idx, "D_VOL", 1.0)
                    reaper.SetTrackSendInfo_Value(other_track, 0, send_idx, "D_PAN", 0.0)
                    -- Post-fader so we hear the member's actual mix output
                    reaper.SetTrackSendInfo_Value(other_track, 0, send_idx, "I_SENDMODE", 0)
                    -- Muted by default (elevated member controls via UI)
                    reaper.SetTrackSendInfo_Value(other_track, 0, send_idx, "B_MUTE", 1)
                    created = created + 1
                    log("Created send: " .. other_name .. " inear -> " .. elevated_name .. " inear (send " .. send_idx .. ")")
                else
                    log("ERROR: Failed to create send: " .. other_name .. " -> " .. elevated_name)
                    errors = errors + 1
                end
            end
        end
    end

    ::continue_elevated::
end

reaper.PreventUIRefresh(-1)
reaper.Undo_EndBlock("Setup mix sends for elevated members", -1)

-- Save project
reaper.Main_SaveProject(0, false)

local result = string.format("OK:created=%d,skipped=%d,errors=%d", created, skipped, errors)
reaper.SetExtState("reaperiem", "mix_sends_result", result, false)
log("Mix sends setup complete: " .. result)
