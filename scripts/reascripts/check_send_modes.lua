-- Check Send Modes Script
-- Verifies send modes on all tracks follow IEM routing rules:
--
--   Input tracks:        all sends must be pre-fader post-FX (mode 3)
--   Member inear tracks: sends to ENGINEER must be post-fader (mode 0)
--                        so the engineer hears each member's actual volume
--
-- Writes result to EXTSTATE: reaperiem/send_mode_check
--   "OK" = all sends have correct mode
--   "FAIL:N" = N sends have wrong mode
--
-- Triggered via HTTP API: /_/_RS_REAPERIEM_CHECK_SENDS

local function is_member_inear(name)
    -- Match "NAME inear" but NOT "ENGINEER inear"
    return name:match(" inear$") and not name:match("^ENGINEER ")
end

local function check_all_sends()
    local num_tracks = reaper.CountTracks(0)
    local bad_sends = 0

    for t = 0, num_tracks - 1 do
        local track = reaper.GetTrack(0, t)
        local _, track_name = reaper.GetTrackName(track)
        local num_sends = reaper.GetTrackNumSends(track, 0)  -- 0 = sends (not receives)
        local expect_post_fader = is_member_inear(track_name)

        for s = 0, num_sends - 1 do
            local current_mode = reaper.GetTrackSendInfo_Value(track, 0, s, "I_SENDMODE")

            if expect_post_fader then
                -- Member inear → engineer: must be post-fader (mode 0)
                if current_mode ~= 0 then
                    bad_sends = bad_sends + 1
                end
            else
                -- All other sends: must be pre-fader post-FX (mode 3)
                if current_mode ~= 3 then
                    bad_sends = bad_sends + 1
                end
            end
        end
    end

    -- Also check master track sends (always pre-fader)
    local master = reaper.GetMasterTrack(0)
    local master_sends = reaper.GetTrackNumSends(master, 0)
    for s = 0, master_sends - 1 do
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
