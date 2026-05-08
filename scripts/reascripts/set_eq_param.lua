-- Set EQ Parameter
-- Sets a single ReaEQ band parameter on a track.
-- Parameters passed via EXTSTATE: reaperiem/eq_set
--   Format: "track=N|band=B|param=P|value=V"
--   param: "freq", "gain", "bw" (normalized 0-1 values)
--
-- Action ID: _RS_REAPERIEM_SET_EQ
-- Result written to EXTSTATE: reaperiem/eq_set_result

local section = "reaperiem"

local function find_reaeq(track)
    local fx_count = reaper.TrackFX_GetCount(track)
    for i = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, i)
        if fx_name:match("ReaEQ") or fx_name:match("^EQ$") or fx_name:match("^EQ ") then
            return i
        end
    end
    return -1
end

local function set_eq()
    local input = reaper.GetExtState(section, "eq_set")
    if not input or input == "" then
        reaper.SetExtState(section, "eq_set_result", "ERROR:no_input", false)
        return
    end

    -- Parse "track=N|band=B|param=P|value=V"
    local track_idx = tonumber(input:match("track=(%d+)"))
    local band = tonumber(input:match("band=(%d+)"))
    local param_name = input:match("param=([%w_]+)")
    local value = tonumber(input:match("value=([%d%.%-]+)"))

    if not track_idx or not band or not param_name or not value then
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:parse_failed:" .. input, false)
        return
    end

    local track = reaper.GetTrack(0, track_idx - 1)
    if not track then
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:track_not_found:" .. track_idx, false)
        return
    end

    local eq_idx = find_reaeq(track)
    if eq_idx < 0 then
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:no_reaeq:" .. track_idx, false)
        return
    end

    -- New "gain_db" branch: caller sends desired dB, we sample ReaEQ's actual
    -- norm↔dB mapping at 21 points and linear-interpolate to find the norm
    -- that yields the desired dB. No mutation during sampling — uses
    -- FormatParamValueNormalized, which is pure-read. Then SetParam with the
    -- interpolated norm.
    if param_name == "gain_db" then
        local function parse_db(fmt)
            return tonumber(fmt:match("(-?[%d%.]+)"))
        end

        local gain_param_idx = band * 3 + 1
        local num_params_g = reaper.TrackFX_GetNumParams(track, eq_idx)
        if gain_param_idx >= num_params_g then
            reaper.SetExtState(section, "eq_set_result",
                "ERROR:gain_param_out_of_range:" .. gain_param_idx, false)
            return
        end

        -- Sample 21 points: norm = 0.00, 0.05, ..., 1.00 → formatted dB.
        local samples = {}
        local N_STEPS = 20 -- 21 sample points, 0.05 norm spacing
        for i = 0, N_STEPS do
            local norm_i = i / N_STEPS
            local _, fmt = reaper.TrackFX_FormatParamValueNormalized(
                track, eq_idx, gain_param_idx, norm_i, "")
            local db_i = parse_db(fmt)
            if db_i == nil then
                reaper.SetExtState(section, "eq_set_result",
                    string.format("ERROR:sample_parse_failed:band=%d,norm=%.3f,fmt=%s",
                        band, norm_i, fmt or ""), false)
                return
            end
            samples[i + 1] = { norm = norm_i, db = db_i }
        end

        -- Linear interpolation: walk samples to find the bracketing pair where
        -- desired_db lies between samples[k].db and samples[k+1].db. ReaEQ's
        -- norm→dB is monotonic increasing within typical bands; if the table
        -- is non-monotonic for a particular band type we still snap to the
        -- closest sample.
        local desired = value
        local best_norm = samples[1].norm
        local best_err = math.huge
        for i = 1, 20 do
            local lo = samples[i]
            local hi = samples[i + 1]
            -- Only interpolate when desired is bracketed AND mapping is locally
            -- increasing (lo.db < hi.db). Otherwise score by absolute distance.
            if lo.db <= desired and desired <= hi.db and lo.db < hi.db then
                local t = (desired - lo.db) / (hi.db - lo.db)
                local n = lo.norm + t * (hi.norm - lo.norm)
                local _, vfmt = reaper.TrackFX_FormatParamValueNormalized(
                    track, eq_idx, gain_param_idx, n, "")
                local v_db = parse_db(vfmt)
                if v_db ~= nil then
                    local err = math.abs(v_db - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = n
                    end
                end
            else
                -- Score by distance to either endpoint
                for _, s in ipairs({ lo, hi }) do
                    local err = math.abs(s.db - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = s.norm
                    end
                end
            end
        end

        reaper.TrackFX_SetParam(track, eq_idx, gain_param_idx, best_norm)

        -- Newton-style refinement loop: read REAPER's actual value after the
        -- interp-based SetParam, nudge norm toward desired using a finite-
        -- difference slope estimate. Converges to REAPER's float32 internal
        -- precision in ≤5 iterations. Same loop applies to gain_db / freq_hz /
        -- bw_oct — only the parse function and TOL differ. (#196 follow-up:
        -- 21-sample interp left ~0.05-unit residual error, visible at 0.1 dB
        -- display rounding.)
        local TOL = 0.05
        local MAX_ITER = 5
        local EPS = 0.001
        for iter = 1, MAX_ITER do
            local _, fmt_now = reaper.TrackFX_GetFormattedParamValue(
                track, eq_idx, gain_param_idx)
            local val_now = parse_db(fmt_now)
            if val_now == nil then break end
            if math.abs(val_now - desired) <= TOL then break end

            -- Probe direction: prefer + EPS; fall back to - EPS only when at upper edge.
            -- (EPS=0.001 ⇒ both clauses can't simultaneously fail; no `else` needed.)
            local probe = (best_norm + EPS <= 1.0) and (best_norm + EPS) or (best_norm - EPS)
            local _, fmt_probe = reaper.TrackFX_FormatParamValueNormalized(
                track, eq_idx, gain_param_idx, probe, "")
            local val_probe = parse_db(fmt_probe)
            if val_probe == nil then break end

            local slope = (val_probe - val_now) / (probe - best_norm)
            if slope == 0 or slope ~= slope then break end -- 0 or NaN

            local adjust = (desired - val_now) / slope
            best_norm = math.max(0.0, math.min(1.0, best_norm + adjust))
            reaper.TrackFX_SetParam(track, eq_idx, gain_param_idx, best_norm)
        end

        local _, fmt_post = reaper.TrackFX_GetFormattedParamValue(
            track, eq_idx, gain_param_idx)
        reaper.SetExtState(section, "eq_set_result",
            string.format(
                "OK:track=%d,band=%d,param=gain_db,desired_db=%.3f,norm=%.6f,formatted=%s",
                track_idx, band, desired, best_norm, fmt_post),
            false)
        return
    end

    -- New "freq_hz" branch: caller sends desired Hz, we sample ReaEQ's actual
    -- norm↔Hz mapping at 21 points and log-space-interpolate to find the norm
    -- that yields the desired Hz. ReaEQ formats freq as "250 Hz" or "1.2 kHz" —
    -- handle both. Same pattern as gain_db (#194).
    if param_name == "freq_hz" then
        local freq_param_idx = band * 3
        local num_params_f = reaper.TrackFX_GetNumParams(track, eq_idx)
        if freq_param_idx >= num_params_f then
            reaper.SetExtState(section, "eq_set_result",
                "ERROR:freq_param_out_of_range:" .. freq_param_idx, false)
            return
        end

        -- Parse Hz from a formatted string like "250 Hz" or "1.2 kHz".
        -- Returns nil if neither form parses cleanly.
        local function parse_hz(fmt)
            local n = tonumber(fmt:match("(-?[%d%.]+)"))
            if n == nil then return nil end
            if fmt:lower():match("khz") then
                return n * 1000.0
            end
            return n
        end

        -- Sample 21 points: norm = 0.00, 0.05, ..., 1.00 → formatted Hz.
        local samples = {}
        local N_STEPS = 20
        for i = 0, N_STEPS do
            local norm_i = i / N_STEPS
            local _, fmt = reaper.TrackFX_FormatParamValueNormalized(
                track, eq_idx, freq_param_idx, norm_i, "")
            local hz_i = parse_hz(fmt)
            if hz_i == nil then
                reaper.SetExtState(section, "eq_set_result",
                    string.format("ERROR:sample_parse_failed:band=%d,norm=%.3f,fmt=%s",
                        band, norm_i, fmt or ""), false)
                return
            end
            samples[i + 1] = { norm = norm_i, hz = hz_i }
        end

        -- Linear interpolation in LOG-Hz space (freq is logarithmic):
        -- find bracketing pair where lo.hz <= desired_hz <= hi.hz, lo.hz < hi.hz.
        -- ReaEQ's norm→Hz is monotonic increasing; if non-monotonic for some
        -- band type we still snap to the closest sample by absolute distance.
        local desired = value
        local best_norm = samples[1].norm
        local best_err = math.huge
        for i = 1, 20 do
            local lo = samples[i]
            local hi = samples[i + 1]
            if lo.hz <= desired and desired <= hi.hz and lo.hz < hi.hz then
                local t = (math.log(desired) - math.log(lo.hz))
                        / (math.log(hi.hz) - math.log(lo.hz))
                local n = lo.norm + t * (hi.norm - lo.norm)
                local _, vfmt = reaper.TrackFX_FormatParamValueNormalized(
                    track, eq_idx, freq_param_idx, n, "")
                local v_hz = parse_hz(vfmt)
                if v_hz ~= nil then
                    local err = math.abs(v_hz - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = n
                    end
                end
            else
                for _, s in ipairs({ lo, hi }) do
                    local err = math.abs(s.hz - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = s.norm
                    end
                end
            end
        end

        reaper.TrackFX_SetParam(track, eq_idx, freq_param_idx, best_norm)

        -- Newton-style refinement loop: read REAPER's actual value after the
        -- interp-based SetParam, nudge norm toward desired using a finite-
        -- difference slope estimate. Converges to REAPER's float32 internal
        -- precision in ≤5 iterations. Same loop applies to gain_db / freq_hz /
        -- bw_oct — only the parse function and TOL differ. (#196 follow-up:
        -- 21-sample interp left ~0.05-unit residual error,
        -- amplified by REAPER's display-formatted Hz precision.)
        local TOL = 0.5
        local MAX_ITER = 5
        local EPS = 0.001
        for iter = 1, MAX_ITER do
            local _, fmt_now = reaper.TrackFX_GetFormattedParamValue(
                track, eq_idx, freq_param_idx)
            local val_now = parse_hz(fmt_now)
            if val_now == nil then break end
            if math.abs(val_now - desired) <= TOL then break end

            -- Probe direction: prefer + EPS; fall back to - EPS only when at upper edge.
            -- (EPS=0.001 ⇒ both clauses can't simultaneously fail; no `else` needed.)
            local probe = (best_norm + EPS <= 1.0) and (best_norm + EPS) or (best_norm - EPS)
            local _, fmt_probe = reaper.TrackFX_FormatParamValueNormalized(
                track, eq_idx, freq_param_idx, probe, "")
            local val_probe = parse_hz(fmt_probe)
            if val_probe == nil then break end

            local slope = (val_probe - val_now) / (probe - best_norm)
            if slope == 0 or slope ~= slope then break end -- 0 or NaN

            local adjust = (desired - val_now) / slope
            best_norm = math.max(0.0, math.min(1.0, best_norm + adjust))
            reaper.TrackFX_SetParam(track, eq_idx, freq_param_idx, best_norm)
        end

        local _, fmt_post = reaper.TrackFX_GetFormattedParamValue(
            track, eq_idx, freq_param_idx)
        reaper.SetExtState(section, "eq_set_result",
            string.format(
                "OK:track=%d,band=%d,param=freq_hz,desired_hz=%.3f,norm=%.6f,formatted=%s",
                track_idx, band, desired, best_norm, fmt_post),
            false)
        return
    end

    -- New "bw_oct" branch: caller sends desired bandwidth in octaves; we
    -- sample 21 norm→oct points and LINEAR-interpolate. ReaEQ formats bw
    -- as e.g. "1.18 oct". Same pattern as gain_db / freq_hz (#196).
    if param_name == "bw_oct" then
        local function parse_oct(fmt)
            return tonumber(fmt:match("(-?[%d%.]+)"))
        end

        local bw_param_idx = band * 3 + 2
        local num_params_b = reaper.TrackFX_GetNumParams(track, eq_idx)
        if bw_param_idx >= num_params_b then
            reaper.SetExtState(section, "eq_set_result",
                "ERROR:bw_param_out_of_range:" .. bw_param_idx, false)
            return
        end

        -- Sample 21 points: norm = 0.00, 0.05, ..., 1.00 → formatted oct.
        local samples = {}
        local N_STEPS = 20
        for i = 0, N_STEPS do
            local norm_i = i / N_STEPS
            local _, fmt = reaper.TrackFX_FormatParamValueNormalized(
                track, eq_idx, bw_param_idx, norm_i, "")
            local oct_i = parse_oct(fmt)
            if oct_i == nil then
                reaper.SetExtState(section, "eq_set_result",
                    string.format("ERROR:sample_parse_failed:band=%d,norm=%.3f,fmt=%s",
                        band, norm_i, fmt or ""), false)
                return
            end
            samples[i + 1] = { norm = norm_i, oct = oct_i }
        end

        -- Linear interpolation in oct space.
        local desired = value
        local best_norm = samples[1].norm
        local best_err = math.huge
        for i = 1, 20 do
            local lo = samples[i]
            local hi = samples[i + 1]
            if lo.oct <= desired and desired <= hi.oct and lo.oct < hi.oct then
                local t = (desired - lo.oct) / (hi.oct - lo.oct)
                local n = lo.norm + t * (hi.norm - lo.norm)
                local _, vfmt = reaper.TrackFX_FormatParamValueNormalized(
                    track, eq_idx, bw_param_idx, n, "")
                local v_oct = parse_oct(vfmt)
                if v_oct ~= nil then
                    local err = math.abs(v_oct - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = n
                    end
                end
            else
                for _, s in ipairs({ lo, hi }) do
                    local err = math.abs(s.oct - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = s.norm
                    end
                end
            end
        end

        reaper.TrackFX_SetParam(track, eq_idx, bw_param_idx, best_norm)

        -- Newton-style refinement loop: read REAPER's actual value after the
        -- interp-based SetParam, nudge norm toward desired using a finite-
        -- difference slope estimate. Converges to REAPER's float32 internal
        -- precision in ≤5 iterations. Same loop applies to gain_db / freq_hz /
        -- bw_oct — only the parse function and TOL differ. (#196 follow-up:
        -- 21-sample interp left ~0.005-unit residual error,
        -- amplified by REAPER's display-formatted oct precision.)
        local TOL = 0.005
        local MAX_ITER = 5
        local EPS = 0.001
        for iter = 1, MAX_ITER do
            local _, fmt_now = reaper.TrackFX_GetFormattedParamValue(
                track, eq_idx, bw_param_idx)
            local val_now = parse_oct(fmt_now)
            if val_now == nil then break end
            if math.abs(val_now - desired) <= TOL then break end

            -- Probe direction: prefer + EPS; fall back to - EPS only when at upper edge.
            -- (EPS=0.001 ⇒ both clauses can't simultaneously fail; no `else` needed.)
            local probe = (best_norm + EPS <= 1.0) and (best_norm + EPS) or (best_norm - EPS)
            local _, fmt_probe = reaper.TrackFX_FormatParamValueNormalized(
                track, eq_idx, bw_param_idx, probe, "")
            local val_probe = parse_oct(fmt_probe)
            if val_probe == nil then break end

            local slope = (val_probe - val_now) / (probe - best_norm)
            if slope == 0 or slope ~= slope then break end -- 0 or NaN

            local adjust = (desired - val_now) / slope
            best_norm = math.max(0.0, math.min(1.0, best_norm + adjust))
            reaper.TrackFX_SetParam(track, eq_idx, bw_param_idx, best_norm)
        end

        local _, fmt_post = reaper.TrackFX_GetFormattedParamValue(
            track, eq_idx, bw_param_idx)
        reaper.SetExtState(section, "eq_set_result",
            string.format(
                "OK:track=%d,band=%d,param=bw_oct,desired_oct=%.3f,norm=%.6f,formatted=%s",
                track_idx, band, desired, best_norm, fmt_post),
            false)
        return
    end

    -- Handle "enabled" param via BANDENABLEDM (NO colon, M=band number).
    -- IMPORTANT: BANDENABLED:N (WITH colon) is a GLOBAL toggle — do NOT use.
    -- BANDENABLEDM (without colon) is the actual per-band enabled checkbox.
    if param_name == "enabled" then
        local en = (value >= 0.5) and "1" or "0"
        reaper.TrackFX_SetNamedConfigParm(track, eq_idx, "BANDENABLED" .. band, en)
        reaper.SetExtState(section, "eq_set_result",
            string.format("OK:track=%d,band=%d,param=enabled,value=%.6f,formatted=%s",
                track_idx, band, value, en == "1" and "enabled" or "disabled"), false)
        return
    end

    -- Calculate parameter index
    -- Each band has 3 params: freq (0), gain (1), bw (2)
    local param_offset = 0
    if param_name == "freq" then param_offset = 0
    elseif param_name == "gain" then param_offset = 1
    elseif param_name == "bw" then param_offset = 2
    else
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:unknown_param:" .. param_name, false)
        return
    end

    local param_idx = band * 3 + param_offset

    -- Validate param index
    local num_params = reaper.TrackFX_GetNumParams(track, eq_idx)
    if param_idx >= num_params then
        reaper.SetExtState(section, "eq_set_result",
            "ERROR:param_out_of_range:" .. param_idx, false)
        return
    end

    -- Set the parameter (normalized 0-1)
    reaper.TrackFX_SetParam(track, eq_idx, param_idx, value)

    -- Read back formatted value for confirmation
    local _, fmt = reaper.TrackFX_GetFormattedParamValue(track, eq_idx, param_idx)

    reaper.SetExtState(section, "eq_set_result",
        string.format("OK:track=%d,band=%d,param=%s,value=%.6f,formatted=%s",
            track_idx, band, param_name, value, fmt), false)
end

local ok, err = pcall(set_eq)
if not ok then
    reaper.SetExtState(section, "eq_set_result", "ERROR:" .. tostring(err), false)
end
