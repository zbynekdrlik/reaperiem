-- Check Send Modes Script
-- Verifies ALL sends on ALL tracks are in pre-fader post-FX mode (I_SENDMODE = 3)
--
-- Writes result to EXTSTATE: reaperiem/send_mode_check
--   "OK" = all sends are mode 3
--   "FAIL:N" = N sends are NOT mode 3
--
-- Used by CI to HARD FAIL the pipeline if any send regresses to post-fader.
-- Triggered via HTTP API: /_/_RS_REAPERIEM_CHECK_SENDS

local function check_all_sends()
    local num_tracks = reaper.CountTracks(0)
    local total_sends = 0
    local bad_sends = 0

    for t = 0, num_tracks - 1 do
        local track = reaper.GetTrack(0, t)
        local num_sends = reaper.GetTrackNumSends(track, 0)  -- 0 = sends (not receives)

        for s = 0, num_sends - 1 do
            total_sends = total_sends + 1
            local current_mode = reaper.GetTrackSendInfo_Value(track, 0, s, "I_SENDMODE")

            if current_mode ~= 3 then
                bad_sends = bad_sends + 1
            end
        end
    end

    -- Also check master track sends
    local master = reaper.GetMasterTrack(0)
    local master_sends = reaper.GetTrackNumSends(master, 0)
    for s = 0, master_sends - 1 do
        total_sends = total_sends + 1
        local current_mode = reaper.GetTrackSendInfo_Value(master, 0, s, "I_SENDMODE")
        if current_mode ~= 3 then
            bad_sends = bad_sends + 1
        end
    end

    local result
    if bad_sends == 0 then
        result = "OK"
    else
        result = "FAIL:" .. bad_sends
    end

    reaper.SetExtState("reaperiem", "send_mode_check", result, false)
end

local ok, err = pcall(check_all_sends)
if not ok then
    reaper.SetExtState("reaperiem", "send_mode_check", "ERROR:" .. tostring(err), false)
end
