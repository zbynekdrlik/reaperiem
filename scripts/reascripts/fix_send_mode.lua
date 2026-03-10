-- Fix Send Mode Script
-- Sets ALL sends on ALL tracks to pre-fader post-FX mode (I_SENDMODE = 3)
--
-- Problem: REAPER defaults sends to post-fader mode. When a member adjusts their
-- main channel fader, it changes the signal level sent to ALL other members' IEM mixes.
-- Sends should be pre-fader so each member's mix is independent.
--
-- This script can be run once to fix an existing project, or triggered via HTTP API.
-- After running, all sends will be pre-fader and each member's mix will be fully independent.
--
-- Mode values:
--   0 = post-fader (post-pan) — DEFAULT, WRONG for IEM
--   1 = pre-fader (pre-FX)
--   3 = pre-fader (post-FX) — CORRECT for IEM mixing

local AUTO_MODE = reaper.GetExtState("reaperiem", "auto_fix_sends") == "1"
if AUTO_MODE then
    reaper.SetExtState("reaperiem", "auto_fix_sends", "0", false)
end

local function log(msg)
    if not AUTO_MODE then
        reaper.ShowConsoleMsg(msg .. "\n")
    end
end

local function fix_all_sends()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    log("========================================")
    log("Fix Send Mode - Pre-Fader Post-FX")
    log("========================================")

    local num_tracks = reaper.CountTracks(0)
    local total_sends = 0
    local fixed_sends = 0

    for t = 0, num_tracks - 1 do
        local track = reaper.GetTrack(0, t)
        local _, track_name = reaper.GetTrackName(track)
        local num_sends = reaper.GetTrackNumSends(track, 0)  -- 0 = sends (not receives)

        for s = 0, num_sends - 1 do
            total_sends = total_sends + 1
            local current_mode = reaper.GetTrackSendInfo_Value(track, 0, s, "I_SENDMODE")

            if current_mode ~= 3 then
                reaper.SetTrackSendInfo_Value(track, 0, s, "I_SENDMODE", 3)
                fixed_sends = fixed_sends + 1
                log(string.format("  Fixed: %s send %d (was mode %d -> now 3)", track_name, s, current_mode))
            end
        end
    end

    -- Also fix master track sends
    local master = reaper.GetMasterTrack(0)
    local master_sends = reaper.GetTrackNumSends(master, 0)
    for s = 0, master_sends - 1 do
        total_sends = total_sends + 1
        local current_mode = reaper.GetTrackSendInfo_Value(master, 0, s, "I_SENDMODE")
        if current_mode ~= 3 then
            reaper.SetTrackSendInfo_Value(master, 0, s, "I_SENDMODE", 3)
            fixed_sends = fixed_sends + 1
            log(string.format("  Fixed: MASTER send %d (was mode %d -> now 3)", s, current_mode))
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.Undo_EndBlock("Fix all sends to pre-fader post-FX", -1)

    log("")
    log(string.format("Total sends: %d", total_sends))
    log(string.format("Fixed sends: %d", fixed_sends))
    log(string.format("Already correct: %d", total_sends - fixed_sends))
    log("========================================")

    -- Write result to EXTSTATE for automated callers
    reaper.SetExtState("reaperiem", "fix_sends_result",
        string.format("OK:fixed=%d,total=%d", fixed_sends, total_sends), false)

    if not AUTO_MODE then
        reaper.ShowMessageBox(
            string.format("Fixed %d of %d sends to pre-fader post-FX mode.", fixed_sends, total_sends),
            "Send Mode Fix Complete",
            0
        )
    end
end

local ok, err = pcall(fix_all_sends)
if not ok then
    if not AUTO_MODE then
        reaper.ShowConsoleMsg("FATAL ERROR: " .. tostring(err) .. "\n")
        reaper.ShowMessageBox("Fix failed!\n\n" .. tostring(err), "Error", 0)
    end
end
