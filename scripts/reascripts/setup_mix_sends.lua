-- setup_mix_sends.lua
-- Creates one-directional inear sends for Petronela's elevated access ONLY.
-- Triggered via: _RS_REAPERIEM_SETUP_MIX_SENDS
--
-- Creates sends FROM each other non-engineer member's inear track
-- TO PETRONELA inear. Sends are muted by default (volume 0dB, muted).
-- Idempotent: skips sends that already exist.
--
-- ONE-DIRECTIONAL ONLY: never creates sends FROM Petronela TO others.
-- Bidirectional sends create REAPER routing loops that silently block all audio.

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

local petronela_track = inear_tracks["PETRONELA"]
if not petronela_track then
    local result = "ERROR:petronela_inear_not_found"
    reaper.SetExtState("reaperiem", "mix_sends_result", result, false)
    log(result)
    return
end

-- Check if a send from src to dst already exists
local function send_exists(src_track, dst_track)
    local num_sends = reaper.GetTrackNumSends(src_track, 0)
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

-- Create sends FROM each other member's inear TO PETRONELA inear (one-way only)
for _, src_name in ipairs(inear_names) do
    if src_name ~= "PETRONELA" and src_name ~= "ENGINEER" then
        local src_track = inear_tracks[src_name]
        if send_exists(src_track, petronela_track) then
            skipped = skipped + 1
        else
            local send_idx = reaper.CreateTrackSend(src_track, petronela_track)
            if send_idx >= 0 then
                reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "D_VOL", 1.0)
                reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "D_PAN", 0.0)
                -- Post-fader so we hear the member's actual mix output
                reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "I_SENDMODE", 0)
                -- Muted by default (Petronela controls via Mixes tab UI)
                reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "B_MUTE", 1)
                created = created + 1
                log("Created send: " .. src_name .. " inear -> PETRONELA inear (send " .. send_idx .. ")")
            else
                log("ERROR: Failed to create send: " .. src_name .. " -> PETRONELA")
                errors = errors + 1
            end
        end
    end
end

reaper.PreventUIRefresh(-1)
reaper.Undo_EndBlock("Setup mix sends for Petronela elevated access", -1)

reaper.Main_SaveProject(0, false)

local result = string.format("OK:created=%d,skipped=%d,errors=%d", created, skipped, errors)
reaper.SetExtState("reaperiem", "mix_sends_result", result, false)
log("Mix sends setup complete: " .. result)
