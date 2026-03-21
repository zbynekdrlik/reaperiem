-- Fix Send Mode Script
-- Sets send modes according to IEM routing rules:
--
--   Input tracks:        pre-fader post-FX (mode 3) — each member's mix is independent
--   Member inear tracks: post-fader (mode 0) — engineer hears member's actual volume
--
-- Mode values:
--   0 = post-fader (post-pan)
--   1 = pre-fader (pre-FX)
--   3 = pre-fader (post-FX)

local AUTO_MODE = reaper.GetExtState("reaperiem", "auto_fix_sends") == "1"
if AUTO_MODE then
    reaper.SetExtState("reaperiem", "auto_fix_sends", "0", false)
end

local function log(msg)
    if not AUTO_MODE then
        reaper.ShowConsoleMsg(msg .. "\n")
    end
end

local function is_member_inear(name)
    -- Match "NAME inear" but NOT "ENGINEER inear"
    return name:match(" inear$") and not name:match("^ENGINEER ")
end

local function is_monitor_bus(track)
    local _, name = reaper.GetTrackName(track)
    return name:lower() == "monitor bus"
end

local function fix_all_sends()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    log("========================================")
    log("Fix Send Modes — IEM Routing Rules")
    log("========================================")

    local num_tracks = reaper.CountTracks(0)
    local total_sends = 0
    local fixed_sends = 0

    for t = 0, num_tracks - 1 do
        local track = reaper.GetTrack(0, t)
        local _, track_name = reaper.GetTrackName(track)
        local num_sends = reaper.GetTrackNumSends(track, 0)  -- 0 = sends (not receives)
        local expect_post_fader = is_member_inear(track_name)
        local target_mode = expect_post_fader and 0 or 3

        for s = 0, num_sends - 1 do
            -- Skip sends to MONITOR bus (always post-fader for listen switching)
            local dest = reaper.GetTrackSendInfo_Value(track, 0, s, "P_DESTTRACK")
            if is_monitor_bus(dest) then
                total_sends = total_sends + 1
            else
                total_sends = total_sends + 1
                local current_mode = reaper.GetTrackSendInfo_Value(track, 0, s, "I_SENDMODE")

                if current_mode ~= target_mode then
                    reaper.SetTrackSendInfo_Value(track, 0, s, "I_SENDMODE", target_mode)
                    fixed_sends = fixed_sends + 1
                    local mode_name = expect_post_fader and "post-fader" or "pre-fader post-FX"
                    log(string.format("  Fixed: %s send %d (was mode %d -> now %d [%s])",
                        track_name, s, current_mode, target_mode, mode_name))
                end
            end
        end
    end

    -- Also fix master track sends (always pre-fader)
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
    reaper.Undo_EndBlock("Fix send modes for IEM routing", -1)

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
            string.format("Fixed %d of %d sends.\n\nInput sends: pre-fader post-FX (mode 3)\nMember inear sends: post-fader (mode 0)",
                fixed_sends, total_sends),
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
