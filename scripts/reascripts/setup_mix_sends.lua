-- setup_mix_sends.lua
-- Creates cross-member inear sends for ALL band members (not just elevated).
-- Triggered via: _RS_REAPERIEM_SETUP_MIX_SENDS
--
-- For each non-engineer member, creates a send FROM each other member's inear track
-- TO this member's inear track. Sends are muted by default (volume 0dB, muted).
-- Idempotent: skips sends that already exist.
--
-- These sends are permanent and always present. The elevated toggle in the
-- IEM mixer app only controls UI visibility — no REAPER routing changes on click.

local function log(msg)
    reaper.ShowConsoleMsg(msg .. "\n")
end

-- Discover all inear tracks
local inear_tracks = {}  -- name -> MediaTrack
local inear_names = {}   -- ordered list of names
local track_count = reaper.CountTracks(0)
for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    local _, name = reaper.GetTrackName(track)
    if name:match(" inear$") or name:match(" INEAR$") then
        local member_name = name:gsub(" [iI][nN][eE][aA][rR]$", "")
        inear_tracks[member_name:upper()] = track
        table.insert(inear_names, member_name:upper())
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

-- For EVERY non-engineer member, create sends from all other members' inear tracks
for _, dest_name in ipairs(inear_names) do
    if dest_name ~= "ENGINEER" then
        local dest_track = inear_tracks[dest_name]

        for _, src_name in ipairs(inear_names) do
            if src_name ~= dest_name and src_name ~= "ENGINEER" then
                local src_track = inear_tracks[src_name]
                if send_exists(src_track, dest_track) then
                    skipped = skipped + 1
                else
                    local send_idx = reaper.CreateTrackSend(src_track, dest_track)
                    if send_idx >= 0 then
                        -- Set volume to 0dB, muted by default
                        reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "D_VOL", 1.0)
                        reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "D_PAN", 0.0)
                        -- Post-fader so we hear the member's actual mix output
                        reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "I_SENDMODE", 0)
                        -- Muted by default (elevated member controls via UI)
                        reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "B_MUTE", 1)
                        created = created + 1
                        log("Created send: " .. src_name .. " inear -> " .. dest_name .. " inear (send " .. send_idx .. ")")
                    else
                        log("ERROR: Failed to create send: " .. src_name .. " -> " .. dest_name)
                        errors = errors + 1
                    end
                end
            end
        end
    end
end

reaper.PreventUIRefresh(-1)
reaper.Undo_EndBlock("Setup cross-member mix sends for all members", -1)

-- Save project
reaper.Main_SaveProject(0, false)

local result = string.format("OK:created=%d,skipped=%d,errors=%d", created, skipped, errors)
reaper.SetExtState("reaperiem", "mix_sends_result", result, false)
log("Mix sends setup complete: " .. result)
