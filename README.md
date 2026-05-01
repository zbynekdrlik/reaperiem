# REAPER IEM Mixing System

MCP server for controlling REAPER as a personal monitor (IEM) mixer for church band.

**URL:** https://iem.newlevel.media/

## Changelog

### v1.163.0 (2026-05-01)

- **CI**: Replaced 6 manual `actions/cache@v4` blocks with `Swatinem/rust-cache@v2`. Fixed broken `build-tauri` cache (was restore-only, never saved artifacts). Standardized auto-pruned target dirs and correctly-keyed cache (rustc + Cargo.lock + features). Cuts ~10–15 min from `build-tauri` after warm-up, ~2–5 min from Ubuntu jobs.

### v1.162.0 (2026-05-01)

- **CI**: Reorder post-deploy steps so REAPER project backup is captured BEFORE the audio-test tone generator is inserted. Prevents the tone-generator FX from being persisted into the saved `.RPP` and resurrected on every restore-after-E2E (root cause of "tone generator stuck on engineer inear track" between deploys).
- **Fix**: `tone_generator.lua` `start` action — when FX is `already_present`, also re-assert `TrackFX_SetEnabled(true)` and `B_MUTE=0` so the script's contract matches its name regardless of pre-state.

### v1.161.0 (2026-04-28)

- **Fix**: "Reconnecting" banner and audio listen button no longer flash on transient WebSocket blips (Wi-Fi handoff, brief tab backgrounding, mobile suspend). 3 s client-side debounce — sustained disconnects (>3 s) still show the message. (#186)

### v1.160.0 (2026-04-28)

- **CI**: Cache npm download cache and Playwright browsers on the GitHub-hosted `e2e` job — saves ~2–3 min per push after the first cache warm-up

### v1.159.0 (2026-04-26)

- **Fix**: Backup/restore — prevent silent partial captures (engineer now sees an error instead of writing an incomplete file)
- **Fix**: Backup/restore — drop `inear`/`stems` filter on track-mute capture so all tracks (incl. CG and other tech tracks) are restored correctly
- **Fix**: Auto-snapshot — flag for "snapshot done today" is now set AFTER successful save (was set before, blocking retry on failure)
- **Feature**: Restore preview now shows "Will NOT be restored" panel listing tracks present in REAPER but missing from the backup
- **CI**: Mutation testing gate on `backup_*` and `snapshot_*` modules; coverage threshold raised to 85% for those modules

### v1.158.0 (2026-04-20)

- **Feature**: New stereo input `CG` (Dante RX 53/54) for content playback (YouTube videos, music) during presentations — routed to all 10 member inears, default-muted. Members unmute individually when content plays.

### v1.157.0 (2026-04-20)

- **Fix**: Engineer "Listen" button — restore binary audio streaming to the browser (regression: server accepted ListenStart but forwarded zero Opus frames, leaving the button stuck on "No Source" after 5 s timeout).
- **CI hardening**: extend `test-integrity` to reject silent-skip patterns in live E2E tests (`console.log("[SKIP]"`, `return;` after `count()`/auth guards, `catch {}` around `waitForFunction`).
- **E2E**: new binary-frames-or-die test `audio-listen-e2e.spec.ts` asserts ≥30 Opus frames within a 3 s ListenStart window against live REAPER with the tone generator active.
- **Diagnostics**: `/api/audio/diagnostics` now reports `frames_forwarded` (count of Opus frames sent on `/ws/audio` since app start) to catch pipeline breaks in production between deploys.

### v1.156.0 (2026-04-19)

- **Fix**: EQ band cards stack vertically on phones in portrait — previously iPhone 17 Pro (430 px viewport) rendered two bands per row, leaving ~58 px of horizontal space for FREQ/Q/GAIN sliders. Cards now go full-width on any touch device in portrait. (#179)
- **Fix**: EQ parameter labels and values now use higher-contrast text — FREQ/Q/GAIN labels went from `#555` to `#bbb`, values went from `#888` to `#eaeaea`.
- **Fix**: EQ row chrome compressed — label width 32→24 px, value width 60→44 px, card padding 12→10 px, row gap 8→6 px. Combined with portrait stacking, slider real estate on iPhone 17 Pro goes from ~58 px to ~290 px.
- **Feature**: Fullscreen movement-mode indicator — when an EQ slider enters active/activating state, the whole modal gets a cyan inset border via CSS `:has()`. Visible regardless of whether haptics are enabled.

### v1.155.0 (2026-04-19)

- **Fix (critical)**: ALEX kl mute / fader / pan were silently dropped at the server — every WS command for track_index=44 failed the `is_valid_track` check because it compared against `inputs.len()` (23) instead of the resolved REAPER track indices. The keyboard was uncontrollable for members during the April live service; REAPER never received the commands even though the UI showed them applied. (#179)
- **Fix**: `validate_track_index` (REST endpoints for level/pan/mute) now also uses the resolved REAPER index set.
- **Test**: `alex-kl.spec.ts` fader + new mute test assert against REAPER HTTP directly (`GET /TRACK/N/SEND/M`), not the UI's optimistic `.db-display`. Bug reproduces deterministically with the old validator.
- **CI**: New scanner `scripts/check_track_index_validator.py` fails CI if any code compares `track_index` / `ti` against `inputs.len()` / `input_count` — prevents silent recurrence.

### v1.154.0 (2026-04-18)

- **Fix**: ALEX kl `I_RECMON=1` — keyboard now passes audio to member IEMs during live services (was silent with RECMON=0).
- **Fix**: ALEX kl sends use `I_SENDMODE=3` (pre-fader post-FX) so TRIM IN + ReaEQ actually affect member mixes.
- **Fix**: ALEX kl → ENGINEER inear send is muted by default, matching the existing `create_sends_for_member` convention — no more keyboard in the engineer's solo bus.
- **Fix**: `is_input_track` (Lua) excludes REAPER folder parents (INPUTS, MICS, TECH, OUTPUTS, BAND) via `I_FOLDERDEPTH>0` — TRIM IN / ReaEQ no longer get inserted on folders.
- **Fix**: Deploy config merger writes atomically (tmp+`os.replace`) so a crash mid-deploy can't wipe `jwt_secret` / `vapid_private_key`.
- **CI**: Explicit `pip install pyyaml` precheck runs BEFORE stopping the app — a missing runner dependency can't leave production offline.
- **Test**: `alex-kl.spec.ts` restores the fader position in a `finally` block so the test doesn't walk ALEX kl toward silence across CI runs.

### v1.153.0 (2026-04-16)

- **Feature**: Added ALEX kl stereo keyboard input (Dante RX 13/14) routable to all band member mixes.
- **Refactor**: `InputTrack` config struct now honors `category` and `stereo_pair` fields from `input_tracks.yaml` (were previously ignored by serde).
- **Refactor**: REAPER FX setup scripts (`setup_input_trim`, `setup_input_eq`, `check_input_trim`) use a category-agnostic `is_input_track` predicate — future instruments need no Lua changes.

### v1.152.0 (2026-04-16)

- **Refactor**: MixerPage decomposed from single 2865-line file into module directory with 7 files (#165)
- **Arch**: `ConnectionManager` with deterministic `Drop` — all background tasks (WebSocket, reconnect, watchdog, token expiry) torn down in one place
- **Arch**: `MixerState` struct — `connect_websocket` reduced from 44 parameters to 7
- **Arch**: `disposal_guard` replaces per-signal scope-alive checks

### v1.151.0 (2026-04-14)

- **CI**: Harden VST3 deploy step against `Remove-Item` file-lock races. After a taskkill, Windows can hold loaded-DLL handles open for several seconds. The previous fixed 3 s `Start-Sleep` was not enough on a loaded runner, causing `Access to the path 'OIEM Receive.vst3' is denied` and red deploys. Now: poll Get-Process up to 30 s before proceeding, and retry `Remove-Item` with exponential backoff (6 attempts, ~31 s total).
- **Fix**: Service Worker now awaits `cache.put` before returning the response. Previously the detached promise could finish after the page's `networkidle`, causing a rare post-reload cache-empty race (observed as `pwa.spec.ts` intermittent flakes on loaded CI runners). Functional impact is minimal — `cache.put` typically completes in under 1 ms.

### v1.150.0 (2026-04-14)

- **Test**: Fix `openKebabMenu` race in `eq.spec.ts` — the helper now waits for the target channel strip (e.g. MIREC) to render before iterating. Previously caused intermittent `No channel found` failures in post-deploy E2E when the Mics tab render was slightly slower than the fixed 300 ms timeout.

### v1.149.0 (2026-04-13)

- **Feature**: Per-inear-track limiter activation counter (#145). Open the LIM dialog on any channel to see how long that inear's safety limiter has been actively reducing gain (e.g. "21.3 sec limited" or "1 min 23 sec limited") since the last reset, plus a Reset button to zero it. Visible to engineer (any track) and to band members on their own track.
- **Note**: Existing limiter instances pick up the new GR readout on next REAPER FX reload (next REAPER restart or project reload).

### v1.148.0 (2026-04-13)

- **Fix**: Talkback audio quality — eliminated "low quality / hanging / not fluent" by adding a 60 ms server-side jitter buffer with 20 ms drain loop, replacing the deprecated ScriptProcessor with an AudioWorklet that emits exact 20 ms Opus frames, and bumping Opus bitrate 64→96 kbps for voice. Addresses #154.
- **Feature**: `/api/talkback/diagnostics` (engineer-only) exposes packets_in, packets_out, seq_gaps, buffer_fill_ms, buffer_overflows, last_packet_age_ms, underruns, bitrate_kbps, recv_vst_addr.
- **Test**: New live Playwright gate `talkback-quality.spec.ts` — fake-audio fixture, REAPER meter polling, asserts continuous signal + no hangs + clean release + sane diagnostics.

### v1.147.0 (2026-04-12)

- **Fix**: EQ visualization — shelving filters (lowshelf/highshelf) no longer ring near their corner frequencies. Rewrote shelf biquad math to use the Audio EQ Cookbook's S-parameterized formula instead of the peaking-EQ Q formula, eliminating the ~1.4 dB overshoot that made neighbouring peaking bands look "oversaturated" versus REAPER's native ReaEQ display (#167).

### v1.146.0 (2026-04-12)

- **Fix**: Vibration reliability in SOS alerts — replaced interval-based single pulses with browser-native pattern vibration + foreground recovery via visibilitychange listener (#162)

### v1.145.0 (2026-04-12)

- **Feature**: LIM button now visible to all band members on their IEM Volume fader, not just the engineer — every member can control their own hearing protection threshold (#156)
- **Security**: Server-side track-ownership validation ensures members can only control the limiter on their own output track; engineer retains control of all tracks (#156)

### v1.144.0 (2026-04-12)

- **Fix**: Eliminated the "tried to access a reactive value that has already been disposed" error that could appear on the Android PWA when navigating back from the member mixer to the member selector. The underlying cause was plain Leptos `.set()` / `.update()` calls racing with component disposal when background intervals, WebSocket callbacks, and `spawn_local` async tasks kept firing during teardown. Production logs showed the panic arriving at ~1 Hz after the user was already on the landing page, because disposed-scope writes aborted the JS tick but did not stop the underlying intervals. (#153)
- **Hardening**: Project-wide sweep of every plain Leptos signal write in `iem-mixer/iem-ui/src/` (~296 call sites across 14 files) to its defensive `try_set` / `try_update` variant. The `try_` variants silently no-op when the target signal has been disposed, which is exactly the desired behavior for background tasks and event handlers. Includes previously-missed sites in `eq_modal.rs` where signals were bound to non-`set_*` local names (`local_state_created`, `any_dragging`, `local_value`, etc.) and `settings_modal.rs` where `Option<WriteSignal>` was destructured into a bare `set` local inside `spawn_local`. (#153)
- **CI gate**: `scripts/check_disposal_safety.py` rewritten into a two-pass context-free rule. First pass collects every `let <name> = RwSignal::new(...)` binding and every `<field>: RwSignal::new(...)` struct initializer. Second pass forbids any plain `.set()` / `.update()` / `.set_untracked()` / `.update_untracked()` on a tracked name or on any `set_*` identifier — no danger-zone tracking, no false negatives on helper functions called from closures. Scanner self-tests grew from 15 to 21 and now explicitly pin the false-positive guard against `Cell::set()` on non-signals. (#153)
- **E2E**: New `iem-mixer/e2e/tests/live/navigation-back-disposal.spec.ts` runs in the post-deploy job against the live system. Covers four navigation scenarios (browser back, in-page back button, mixer → mixer for a different member, and a 3-iteration mixer-landing loop) and asserts three oracles per scenario: no panic overlay, no `console.error` / `console.warn` messages, and no POST to `/api/client-error` during a 3-second settling window.
- **Architectural follow-up**: GitHub issue #165 tracks the longer-term refactor to a `ConnectionManager` struct with explicit disposal-guard, deferred as not-blocking once the sweep made the current code disposal-safe in every reachable path.

### v1.143.0 (2026-04-11)

- **Fix**: comprehensive hardening of Leptos reactive-disposal races. v1.142.0's new WASM panic hook immediately surfaced a pre-existing bug class in production: `spawn_local` async tasks that wrote to signals after an `await` could panic with "tried to access a reactive value that has already been disposed" when the component unmounted mid-task. This release converts **54 signal-write sites** across 9 files (`mixer.rs`, `talk_button.rs`, `backup_section.rs`, `audio_player.rs`, `pin_change_modal.rs`, `preset_modal.rs`, `snapshot_modal.rs`, `landing.rs`, `login.rs`) to use `try_set` / `try_update`, which silently no-op on disposed signals instead of panicking. (#153)
- **CI gate**: new `scripts/check_disposal_safety.py` check in the test-integrity job scans every `.rs` file in `iem-mixer/iem-ui/src/` and fails the build if any new `set_*.set()` / `set_*.update()` call appears inside a `spawn_local(async ...)` block without the `try_` prefix. Prevents this class of bug from ever coming back. Escape hatch via `// disposal-safe: <reason>` comment for the rare legitimate case. (#153)

### v1.142.0 (2026-04-11)

- **Fix**: PWA self-healing — a new 5-second WebSocket watchdog force-closes sockets that have received no frames for more than 30 seconds. Catches "zombie sockets" where `readyState` is OPEN but no data flows. After force-close, the existing `.disconnected-banner` shows and the reconnect loop opens a new socket. Reconnect now uses exponential backoff (2s → 4s → 8s → 15s → 30s cap) instead of the previous unbounded 2-second polling — gentler on mobile radios and battery. (#153)
- **Feature**: WASM panic hook. When the Leptos/Rust frontend panics, a custom hook now renders a red full-screen reload banner into `document.body` (via raw DOM so it survives a broken reactive graph) and fire-and-forgets a POST to the new public `/api/client-error` endpoint. Diagnostics include version, git hash, URL, user-agent, panic message, and source location. Server-side reports are logged via `tracing::warn!` with a grep-able `client_error` prefix and land in the existing rolling log at `%APPDATA%\iem-mixer\logs\iem-mixer.log.YYYY-MM-DD`. Converts silent freezes into inspectable errors. (#153)

### v1.141.0 (2026-04-11)

- **Fix**: SOS alert button now shows the active (red, pulsing) state on the clicking member's own device (#150). On WebSocket connect, the server now catches up a non-engineer member's own active alert state, mirroring the existing engineer catch-up. Previously, reloading the page after triggering SOS left the member's UI stuck in idle while the server still held their alert, and clicking SOS again hit a no-op short-circuit in the CallEngineer handler so the button could never return to active. Restored the `toHaveClass(/active/)` assertion in `alert.spec.ts` as a regression guard.

### v1.140.0 (2026-04-10)

- **CI**: Mutation testing hardening — raised `cargo-mutants --timeout` from 120s to 300s after observing CPU contention under `--jobs 4` on ubuntu-latest runners. Simplified `iem_core::is_valid_pan` by removing a redundant `is_finite()` check (`Range::contains` already rejects NaN and infinities).
- **Docs**: Fixed stale comment in `test_reaper_pan_to_ui_nan_maps_to_center` that referenced a removed helper function. Updated the mutation testing spec and marked the plan as superseded by the final implementation.

### v1.139.0 (2026-04-10)

- **CI**: Added `cargo-mutants` test quality gate. Mutation testing runs on every dev push and PR, mutating only code changed vs `origin/main` (`--in-diff`). Any surviving mutant fails CI. Covers `iem-core` and `iem-server`. Catches weak tests that exercise code without verifying behavior.

### v1.138.0 (2026-04-10)

- **Feature**: Solo indicator in header — when solo is active, a prominent yellow "SOLO ✕" button replaces the version display in the header, visible on every tab. One click clears solo from any tab.
- **UI**: Header compacted — smaller back button and vertical LAN/WAN indicator to save horizontal space

### v1.137.0 (2026-04-09)

- **Fix**: Solo no longer leaves tracks muted after PWA crash/disconnect (#155)
- **Feature**: Server-managed solo state persists across reconnects — solo stays active until explicitly turned off

### v1.136.0 (2026-04-09)

- **Feature**: Backup/restore system — automatic scheduled backups at 13:00 and 21:00, engineer-only restore UI in Settings modal with preview and estimated time
- **Feature**: Track-level mute state (global mute, stems mute) now included in backups and restored correctly
- **Fix**: Backup stores LINEAR volumes directly (no dB conversion) — prevents zeroing sends on restore
- **Fix**: Restore skips unchanged values (sends, volumes, EQ, limiter, customizations, PINs) — faster restores
- **Fix**: Estimated restore time includes 30s base read overhead for accurate predictions

### v1.130.0 (2026-04-04)

- **Feature**: Member profile photos — upload from Settings modal, displayed as circular avatars on landing page (#16)
- **Feature**: Client-side photo resize (128×128 center-crop JPEG) — no size restrictions for users
- **Feature**: Photo API with auth (members own photo, engineers any member)

### v1.129.0 (2026-04-03)

- **Fix**: Tauri build timeout increased to 30 min (cold cargo cache on windows-latest exceeds 20 min)
- **Chore**: Gitignore brainstorm artifacts and debug screenshots from repo

### v1.128.0 (2026-04-01)

- **Feature**: Pre-WASM app shell — instant loading screen with spinner before ~10s WASM download
- **Feature**: Service worker cache-first for content-hashed WASM/JS assets (instant repeat loads)
- **Feature**: Token expiry extended from 24 hours to 7 days (PIN entry once a week)
- **Fix**: Robocopy exit code handling in WASM deploy step (exit 3 is success on Windows)

### v1.127.0 (2026-04-01)

- **Fix**: Push subscription POST now awaited directly (was silently dropped by nested spawn_local)
- **Fix**: Old push subscription unsubscribed before resubscribing (required when VAPID key changes)
- **Fix**: Deploy no longer overwrites config.yaml — preserves VAPID key and JWT secret across deploys
- **Fix**: base64url decode corrected for Latin-1 atob output (chars not bytes)
- **Fix**: CDN-Cache-Control no-store for sw.js prevents Cloudflare caching stale service worker

### v1.126.0 (2026-04-01)

- **Fix**: CI deploy jobs no longer get cancelled — cross-workflow runner concurrency group serializes self-hosted runner usage
- **Fix**: Nightly backup timeout added (10 min) to prevent runner monopolization

### v1.125.0 (2026-03-31)

- **Feature**: Web Push notifications for SOS alert — engineer gets notified even when app is closed or screen is off (#133)
- **Feature**: VAPID P-256 key auto-generation for Web Push (pure-Rust crypto, no OpenSSL)

### v1.124.0 (2026-03-31)

- **Fix**: PWA app freezing on phones — eliminated memory leaks from `Closure::wrap().forget()` in Effects
- **Fix**: AudioContext `onstatechange` handler cleared before close (prevents ~2MB leak per listen/stop cycle)
- **Fix**: Visibility listener moved to component body (was stacking on every WebSocket reconnect)
- **Fix**: Meters skipped when page is backgrounded — prevents freeze on tab resume
- **Fix**: `Closure::once().forget()` replaced with `Closure::once_into_js()` (auto-deallocates after fire)

### v1.123.0 (2026-03-31)

- **Feature**: Engineer talk button — push-to-talk to speak to band members via IEM from phone/laptop (#123)
- **Feature**: OIEM Receive VST3 plugin — receives talkback audio on ENGINEER mic track, mixes with Dante mic
- **Feature**: Red pulsing page overlay when Talk is active — vibration on engineer, visual on all devices
- **Feature**: "ENGINEER SPEAKING" banner on band member devices when engineer talks
- **Feature**: SOS alert now shows red pulsing page overlay on engineer devices
- **Feature**: One-at-a-time talkback lock — only one engineer can talk at once
- **Fix**: OIEM port migrated from 6980 to 7980 (avoids VB-Matrix/VBAN range conflict)
- **Fix**: Mute All button shrunk to icon-only for better toolbar layout
- **Closed**: #123 (engineer talk button)

### v1.122.0 (2026-03-30)

- **Feature**: Solo exclusive mode — clicking solo on a new track desolos the previous one instead of appending (#131)
- **Feature**: Band member SOS alert button — calls engineer for help with persistent notification until cleared (#125)
- **Feature**: Engineer alert toast with vibration loop (500ms/1.5s), subtle chime sound, and system notification via service worker
- **Feature**: Alert persists until engineer or member explicitly dismisses — no auto-timeout
- **Feature**: Engineer reconnect catches up on active alerts (no missed SOS)
- **Fix**: CI concurrency — push and PR runs no longer cancel each other's Tauri build
- **Closed**: #131 (solo exclusive), #125 (alert button)

### v1.121.0 (2026-03-29)

- **Fix**: Listen mode stop button now works — previously auto-reconnect overrode user stop, making it impossible to disable listening
- **Fix**: Audio restored when listening on Petronela's page — missing REAPER send from PETRONELA inear to ENGINEER inear recreated
- **Fix**: CI Tauri build no longer hangs on post-cache step (switched to cache/restore)

### v1.120.0 (2026-03-28)

- **Fix**: Routing loop bug — cross-member inear sends created bidirectional loops that REAPER silently blocked, preventing ALL audio from reaching member inear tracks. Replaced with one-directional sends TO Petronela only (#121)
- **Fix**: Listen mode auto-reconnect — audio WebSocket now retries forever on disconnect instead of resetting button to idle. Exponential backoff (1-8s), AudioContext stays alive for seamless resume
- **Fix**: Listen mode server-side mute restoration — REAPER send mutes always restored on WebSocket disconnect (prevents orphaned mute states)
- **Fix**: Adaptive jitter buffer — grows from 80ms to 500ms on dropout, shrinks slowly when stable. Reduces audible drops on bad networks
- **Fix**: Opus FEC enabled on VST encoder — each packet now contains redundant data from previous frame for loss recovery
- **Simplify**: Elevated member access hardcoded to Petronela only (removed dynamic toggle, ElevatedStore, API endpoints, settings UI)
- **Closed**: #55 (app upgrade — solved by CI auto-deploy), #122 (elevated access — implemented v1.117.0)

### v1.118.0 (2026-03-28)

- **Fix**: Engineer token expiry extended from 4 hours to 24 hours — no more repeated PIN entry during rehearsal/service sessions (#124)
- **Fix**: Nightly backup now force-saves REAPER project before collecting files — backed-up RPP is always current (#126)
- **Fix**: Backup worktree creation no longer fails when REAPER has unsaved project changes (added `--force` flag)
- **Fix**: Backup error handling — worktree creation failures now detected and reported instead of silently committing to wrong branch
- **Closed**: #124 (engineer auth too short), #126 (daily backup broken)

### v1.117.0 (2026-03-27)

- **Feature**: Elevated member access — engineer can grant specific band members the Mixes tab to view/control other members' IEM mixes through physical Dante headphones (#122)
- **Feature**: Engineer toggle in member settings panel to set/remove elevated access
- **Feature**: Parametric EQ with HPF/LPF, symmetric -12/+12dB gain slider, consistent reset defaults (#119)
- **Fix**: EQ gain dB mismatch (web app +12dB vs REAPER +6dB) — corrected non-linear gain/freq mapping
- **Fix**: Reset button now moves all sliders, gain slider no longer auto-enables disabled bands
- **Fix**: Preset/snapshot restore now preserves EQ band enabled state
- **Fix**: EQ read race condition (added eq_read_lock for EXTSTATE serialization)
- **Infra**: Pre-created cross-member inear sends for all members (72 sends, muted by default)
- **Infra**: MCP-first mandate in CLAUDE.md — all REAPER operations must use MCP tools
- **Closed**: #9 (mic trim + EQ done, stems trim not needed)

### v1.101.0 (2026-03-23)

- **Feature**: Auto-insert "TRIM IN" (JS:Volume/Pan) as first FX on all mic/guitar tracks for input level normalization (#9)
- **Feature**: MCP tool `list_track_fx` to query trim state on mic/gtr tracks
- **Fix**: CI deploy now uses dynamic action IDs for ReaScript execution (no REAPER restart needed)
- **Closed**: #113 (audio stability — solved in v1.93-v1.99)

### v1.100.0 (2026-03-23)

- **Fix**: Stems group fader now controls audio — changed stems bus send mode from pre-fader to post-fader so the group volume fader affects audio reaching inear tracks (#116)
- **Fix**: CI deploy now runs `fix_send_mode.lua` before verification to apply current routing rules to existing REAPER sends

### v1.99.0 (2026-03-23)

- **Fix**: Corrupted audio on phones — LagrangeInterpolator buffer overread and return value misuse in VST processBlock caused uninitialized memory fed to Opus encoder
- **Fix**: Audio latency display now shows actual scheduling gap instead of fake adaptive jitter value
- **Fix**: 500ms drift cap prevents buffer from growing to 3-4 seconds (mute response time capped)
- **Fix**: Broadcast channel reduced 64→8 frames, VBAN ring buffer 1s→50ms for lower burst latency
- **Fix**: CI deploy schtasks quoting for REAPER paths with spaces, audio engine warmup wait

### v1.92.0 (2026-03-23)

- **Fix**: Stems fader on Main tab now renders after ME mic channel (was incorrectly before it)

### v1.91.0 (2026-03-23)

- **Feature**: Stems group volume fader — control all stem channels (DRUMS, BASS, INST, CLICK, GUIDE, BGVS, OTHER) together with a single fader while preserving individual relative mix levels (#87)
- **Feature**: Stems fader visible on both Main and Stems tabs for quick access
- **Feature**: Stems volume saved/restored with presets (backwards-compatible with old presets)
- **Fix**: CI deploy now reliably kills app processes across sessions (multi-method fallback)
- **Fix**: CI deploy picks correct version installer when multiple are present

### v1.90.0 (2026-03-22)

- **Fix**: Preset names now accept digits and backspace works correctly — login page keyboard listener was leaking globally and intercepting keys on all pages (#110)

### v1.89.0 (2026-03-22)

- **Fix**: Band member Listen now works — mutes other member sends to ENGINEER for true audio isolation (solo-based approach didn't affect send routing)
- **Fix**: Listen mute states fully restored after stopping Listen on band member pages
- **Fix**: CI deploy hardened — REAPER restart verifies project loaded (NTRACK > 0), creates StartREAPER task with correct project path, fails deploy if REAPER doesn't come back

### v1.79.0 (2026-03-21)

- **Feature**: Listen volume boost setting (0-24 dB in 3 dB steps) in engineer Settings modal
- **Fix**: Listen boost applies immediately while listening — no need to stop and restart Listen mode
- **Feature**: Keyboard PIN entry on desktop — type digits directly instead of tapping number pad

### v1.75.0 (2026-03-20)

- **Feature**: Solo state now syncs across devices — solo a channel on your phone and your laptop shows it too
- **Feature**: New connections receive current solo state immediately (no stale UI on second tab)

### v1.74.0 (2026-03-20)

- **Fix**: Phones no longer stuck on infinite spinner after app restart — JWT signing key now persists to config.yaml so cached tokens remain valid across restarts
- **Fix**: Stale tokens auto-detected — after 3 consecutive WebSocket failures, the app verifies the token with the server and redirects to login if rejected (instead of spinning forever)

### v1.73.0 (2026-03-20)

- **Security**: REAPER proxy endpoint now requires engineer authentication
- **Security**: JWT secret auto-generated at startup when not configured (with warning)
- **Security**: Member ID validated against path traversal in all file stores
- **Fix**: MCP meter readings corrected — was dividing dB\*10 by 100 instead of 10 (10x error)
- **Fix**: REST endpoints now use REAPER-discovered members instead of static config
- **Fix**: Batch Reset uses name-based track lookup instead of sequential indices
- **Fix**: WebSocket closure memory leak on reconnect (closures stored instead of forgotten)
- **Perf**: Memoized channel display list to avoid recomputation on every meter update
- **Perf**: E2E tests use pre-built binary (30s startup vs 120s)
- **Robustness**: Atomic file writes (tmp+rename) in all JSON stores prevent corruption on crash
- **Robustness**: Poisoned mutex handled gracefully in audio diagnostics
- **CI**: Nightly backup uses git worktree (no working tree modification while REAPER runs)
- **CI**: Cargo cache cross-job fallback with restore-keys

### v1.72.0 (2026-03-20)

- **Hardening**: App now retries member discovery when REAPER is temporarily unavailable at startup — engineer mix controls auto-recover within 10 seconds instead of staying broken for the entire session
- **Fix**: Engineer mix monitoring now uses post-fader sends so the engineer hears members' actual output volumes (fader adjustments reflected in real-time)
- **Perf**: Fixed PWA freezing on Android — consolidated meter animations, throttled WebSocket updates, added server-side change detection to reduce unnecessary broadcasts

### v1.71.0 (2026-03-19)

- **Fix**: Blank page after deploy — removed service worker caching that served stale WASM/JS assets; all band members' phones auto-fix on next app open (no manual cache clear needed)
- **Fix**: AudioData.copyTo RangeError — use allocationSize + f32-planar format for correct buffer sizing
- **Fix**: Mobile audio playback — AudioContext.resume() now called during user gesture to unblock audio on mobile browsers

### v1.59.0 (2026-03-16)

- **Fix**: Mute/fader controls no longer target wrong member after track insertion/removal — frontend now detects track index shifts and fully replaces channel state instead of merging by stale index
- **Fix**: `<For>` key changed to compound (name + track_index) so Leptos destroys stale closures when tracks shift, preventing captured values from targeting the wrong REAPER track

### v1.58.0 (2026-03-15)

- **Fix**: Engineer mixer now shows all member mix faders — hardware output destination (-1) was breaking send discovery, preventing mix channels from appearing
- **Fix**: Engineer mute no longer shuts down member hardware outputs — mute now targets the correct mix send index instead of hardcoded send 0
- **Fix**: Rate-limited REAPER discovery requests (50ms delay) to prevent HTTP API crashes on startup
- **Fix**: Removed test_setup.lua script that could create random tracks in REAPER
- **Fix**: CI backup step now uses git worktrees to prevent project file deletion

### v1.56.0 (2026-03-14)

- **Fix**: Fader now reaches exact whole-number dB values (e.g., -4.0) — switched to 0.2 dB steps with integer boundary snapping
- **Fix**: Bottom toolbar (Mute All, Snapshots, Presets) no longer disappears on mobile when address bar shows/hides

### v1.54.0 (2026-03-14)

- **Fix**: Mute All button on engineer mixer now mutes all 31 channels (previously only muted 22 input channels, leaving 9 mix channels unmuted)

### v1.52.0 (2026-03-14)

- **Feature**: Auto-redirect — returning users skip the member grid and go straight to their mixer (valid token) or PIN login (expired token)
- **UX**: Back button still works — navigating back shows the member grid within the same session

### v1.51.0 (2026-03-13)

- **Fix**: Channel name truncation — long names like "Petronela" no longer get cut off when muted or stereo-paired (replaced `border-left` with `box-shadow: inset`)

### v1.49.0 (2026-03-13)

- **Feature**: Engineer Mixes tab — monitor each band member's in-ear mix with individual faders
- **Fix**: All engineer channels default-muted (engineer unmutes selectively)
- **Fix**: CI backup handles dirty REAPER project without failing deploy

### v1.47.0 (2026-03-11)

- **Fix**: Engineer PIN change — engineers on member phones can now change the member's PIN (was returning 403)
- **Fix**: Token expiry enforcement — expired tokens are now detected every 60s and redirect to login (was silently failing)
- **UI**: PIN change modal hides "Current PIN" field for engineers (they don't know the member's PIN)

### v1.46.0 (2026-03-11)

- **Security**: Enforce member access control — members can only access their own mixer, engineers can access any (Issue #77)
- **Fix**: Hide button now works on muted channels (Issue #78)
- **Fix**: Cross-member navigation redirects to login page with correct member pre-selected instead of blinking back to landing

### v1.44.0 (2026-03-10)

- **Fix**: Applied pre-fader sends to REAPER — 199 sends corrected from post-fader to pre-fader post-FX mode (Issue #7)
- **CI**: Added "Verify send modes" step that fails pipeline if any send regresses to post-fader
- **Script**: New `check_send_modes.lua` for automated send mode verification via EXTSTATE

### v1.43.0 (2026-03-10)

- **Fix**: Own channel always appears first on Main tab (above pinned channels)
- **Fix**: Removed "MY MIC" label clutter from Main tab
- **Fix**: Kebab menu visual polish — wider dB column, menu positioned left of label
- **Fix**: Channel header dB overflow, kebab visibility, name truncation improvements

### v1.36.0 (2026-03-09)

- **Feature**: Server-side presets — presets now sync across all devices (#70)
- **Feature**: Nightly git backup of snapshots and presets (#64, #71)
- **Feature**: CI deploy backs up snapshots and presets to git repository

### v1.35.0 (2026-03-08)

- **Fix**: Remove explorer-killing icon cache clear from CI deploy (was causing taskbar crash-loop on iem.lan)
- **Fix**: Headphones icon anti-aliased rendering with correct headband arc direction

### v1.34.0 (2026-03-08)

- **Fix**: Version/datetime text contrast improved for better readability (#63)
- **Fix**: Snapshot history shows absolute date with Slovak day name first (#69)
- **Fix**: App icon shows headphones matching tray icon instead of blue rectangle (#2)
- **Feature**: Band changelog skill for user-oriented Slovak changelogs

### v1.33.0 (2026-03-07)

- **Feature**: Access from mobile data via Cloudflare Tunnel - single URL works everywhere
- **Fix**: Tray menu shows correct HTTPS URL

### v1.32.0 (2026-03-06)

- **Feature**: `rename_track` MCP tool for renaming REAPER tracks

### v1.31.0 (2026-03-06)

- **Fix**: Member sees own fader in main section (#51)
- **Fix**: Higher contrast for version/datetime text (#63)
- **Fix**: Comprehensive name changes across REAPER, Dante, and mixer

### v1.30.0 (2026-03-06)

- **Feature**: REAPER as single source of truth for band members
- **Feature**: Version and datetime displayed on landing page
- **Feature**: Global volume persistence across page reloads

### v1.28.0 (2026-03-04)

- **Feature**: Daily preset snapshots - automatic server-side backups of mixer settings
- **Feature**: Snapshot history modal with restore, pin, and delete
- **Feature**: Network error UX improvements with clear feedback
- **Feature**: PIN re-authentication for sensitive operations
- **Fix**: Preset modal responsive on mobile devices
- **Security**: Constant-time PIN comparison

### v1.27.0 (2026-03-03)

- **Feature**: NEWLEVEL IEM MIXER branding
- **Feature**: New app icon

### v1.25.0 (2026-03-02)

- **Fix**: Silent meter bridge (removed console window popup)
- **Fix**: Auto-restart meter bridge on REAPER reconnect

### v1.23.0 (2026-03-02)

- **Feature**: ReaScript meter bridge for true L/R stereo peaks
- **Fix**: Meters show correct L/R stereo levels
- **Fix**: Meters display raw input levels (not affected by fader/pan)
- **Fix**: Correct dB×10 conversion formula

### v1.21.0 (2026-03-01)

- **Feature**: Settings modal with configurable options
- **Setting**: Fader double-tap toggle (enable/disable double-tap to 0 dB)
- **Feature**: Pan slider smooth animation with double-tap to center
- **Feature**: Rename band members from UI
- **Feature**: Change PIN from Settings modal
- **Feature**: Logout button in Settings modal
- **UI**: Category tabs: Main, Mics, Stems, Tech
- **UI**: Global volume (master) fader on Main tab
- **UI**: Presets modal - save/load/delete named presets with timestamps

### v1.20.0 (2026-03-01)

- **Feature**: Full codebase security review (P0+P1+P2 fixes)
- **Feature**: WebSocket real-time communication (replaced HTTP polling)

## Features

- HTTP-based control of REAPER tracks and sends
- Per-band-member "More Me" web interface
- Git version control of REAPER projects via SSH
- Claude Code integration via MCP

## Architecture

- **MCP Server**: Python + FastMCP on dev machine
- **REAPER**: Running on iem.lan with Web Interface enabled
- **Control**: HTTP Web API (port 8080)
- **Version Control**: Git on iem.lan via SSH

## Quick Start

```bash
# Install dependencies
pip install -e ./mcp/reaperiem_mcp

# Configure
cp config/reaper_config.yaml.example config/reaper_config.yaml
# Edit with your settings

# Run MCP server
python -m reaperiem_mcp.server
```
