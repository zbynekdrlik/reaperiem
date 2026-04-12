# Deeply Fix Navigation-Back Disposal Race — Design

**Status:** approved
**Date:** 2026-04-12
**Issue:** follow-up to #153 (user-reported: PWA shows "tried to access a reactive value that has already been disposed" error page after navigating back from mixer to member selector)
**Related:** #165 (deferred architectural alternative)

## Problem

After the v1.143.0 hardening PR (#164), a band-member navigating back from `/<member>` to `/` in the Android PWA still sees the panic-hook error page. The panic message is the generic Leptos "Tried to access a reactive value that has already been disposed."

The iem.lan production log at `%APPDATA%\iem-mixer\logs\iem-mixer.log.YYYY-MM-DD` shows the same panic arriving from the client at roughly 1 Hz — **a continuous stream, not a single event** — always with `url="/"`, always from `reactive_graph-0.1.8/src/traits.rs:355:29` (the generic `Set::set` entry point). The client keeps panicking on every poller tick after the user is already on the landing page.

## Root cause

`iem-mixer/iem-ui/src/pages/mixer.rs:149` contains an unguarded plain signal write inside the `connect_websocket` helper:

```rust
set_ws.set(Some(ws.clone()));
```

`connect_websocket` is called from two places:

1. The initial `Effect::new` at line 806 (synchronous on mount).
2. The reconnect `Closure::wrap` at line 867 (fires from a 2-second JS `setInterval`).

When the user navigates back from `/<member>` to `/`, `MixerPage` starts tearing down, but the reconnect interval (and the 5-second watchdog interval, and the 60-second token-expiry interval) are still live in the JS event loop until their corresponding `on_cleanup` handlers run and clear them. A tick that was already queued in the JS event loop fires one more time. It finds the WebSocket `CLOSED`, calls `connect_websocket(...)`, and hits line 149: `set_ws.set(...)` on a signal whose scope is already being disposed. Panic.

The panic aborts that JS tick but does **not** stop the `setInterval`. The next tick (~2 seconds later) does the same thing. And the 1-Hz cadence seen in the logs is not the 2-second reconnect interval — it's the fact that the panic hook also runs WASM initialization paths that fire additional reactive reads, producing a cascade. Regardless of which exact interval is driving it, the structural cause is the same: a helper function called from a background task writes a reactive signal with plain `.set()` instead of `.try_set()`.

The existing CI scanner (`scripts/check_disposal_safety.py`, v1.143.0) catches plain writes **inside** `spawn_local` / `Closure::wrap` danger zones but not writes in helper functions called **from** those zones. Line 149 sits in a helper whose caller is in a danger zone, so the scanner did not flag it.

## Scope of the fix ("approach B")

Treat every plain `.set()` / `.update()` / `.set_untracked()` / `.update_untracked()` on a Leptos `WriteSignal` as unsafe by default, everywhere in `iem-mixer/iem-ui/src/`. The argument for this strict stance is:

- The `try_` variants silently no-op when the target signal is disposed, which is the exact behavior we want defensively.
- The syntactic cost is a `let _ =` prefix; project code already uses this style throughout the `onmessage` handler in `mixer.rs`.
- A context-free CI rule ("no plain `set_x.set(...)` anywhere") is simpler and more robust than the current context-sensitive danger-zone scanner, and it eliminates the class of bugs where a helper function's callers determine whether a write is "safe."
- Approach C (architectural restructuring with a `disposal_guard` and a `ConnectionManager` struct) is strictly better engineering but offers no additional correctness over approach B. It is deferred to issue #165.

## Fix mechanics

### 1. Version bump

Version goes from `1.143.0` to `1.144.0`. Bump the six standard files (`iem-mixer/Cargo.toml`, `iem-mixer/crates/iem-core/Cargo.toml`, `iem-mixer/crates/iem-server/Cargo.toml`, `iem-mixer/iem-ui/Cargo.toml`, `iem-mixer/src-tauri/Cargo.toml`, `iem-mixer/src-tauri/tauri.conf.json`) as the first commit on `dev`.

### 2. Project-wide sweep

A Python transformer script (kept out of the repo; run once locally) walks `iem-mixer/iem-ui/src/**/*.rs` and converts every occurrence of `set_\w+\.(set|update|set_untracked|update_untracked)\(` into `let _ = set_\w+\.try_\1(`. The transformation preserves indentation and the rest of the expression. Manual review is required for any site where the write is used as an expression (e.g. assigned, chained), but in this codebase no such site exists — the write is always a statement.

Known manual cases:

- `set_data_pulse.update(|v| *v = !*v)` style writes where the closure argument is read. These become `let _ = set_data_pulse.try_update(|v| *v = !*v)` — identical shape, only the prefix changes.
- Multi-line rustfmt-split writes of the form `set_alert_data\n    .set(Some(x))` — the transformer must handle these via the same `MULTILINE_WRITER_TAIL` + `MULTILINE_METHOD_HEAD` logic as the scanner.

After the sweep, the `connect_websocket` helper at `mixer.rs:149` is no longer a special case — it is swept along with everything else.

### 3. Scanner rewrite (`scripts/check_disposal_safety.py`)

Replace the current brace-depth-tracked "danger zone" scanner with a context-free regex scanner:

- **New rule:** any line matching `\bset_\w+\s*\.\s*(set|update|set_untracked|update_untracked)\s*\(` is a violation. The `\b` word boundary ensures `try_set` and `try_update` do not match (the `_` in `try_` is a word character, so `\btry_set` satisfies `\bset_` only if we strip the `try_` — the boundary prevents that).
- **Multi-line:** keep `MULTILINE_WRITER_TAIL` + `MULTILINE_METHOD_HEAD` to catch rustfmt-split writes.
- **Comments:** keep `//` line comment skipping.
- **Strings:** keep the `_strip_strings` helper to avoid false positives on string literals like `"set_foo.set(1)"`.
- **Escape hatch:** keep `// disposal-safe:` for the rare case a human reviewer wants to whitelist a specific line.
- **Delete:** `DANGER_ZONE_START`, brace-depth tracking, the `danger_depth_stack`, and all the branching on "inside/outside zone."

The scanner becomes a ~60-line single-pass regex match per file. Faster, simpler, harder to have false negatives.

### 4. Scanner self-tests (`scripts/test_check_disposal_safety.py`)

Rewrite to match the new rule. The existing "inside vs outside danger zone" test pairs collapse:

- **Flagged (positive cases):** `set_x.set(1)` in any context — top-level fn, `on:click` closure, `Callback::new`, `spawn_local`, `Closure::wrap`, `Effect::new`, `Memo::new`. All eight should be flagged.
- **Not flagged (negative cases):** `set_x.try_set(1)`, `set_x.try_update(|v| *v = 1)`, web_sys method calls like `window.set_interval_with_callback(cb, 1000)` and `opts.set_body(&value)`, writes inside `//` comments, writes inside `"..."` string literals, writes with a `// disposal-safe:` trailer.
- **Multi-line rustfmt-split:** positive case for plain `.set(`, negative case for `.try_set(`.

The test suite grows from 15 tests to ~14 tests (a few redundant pairs collapse, a few new context cases are added).

### 5. E2E test against live system (`iem-mixer/e2e/tests/live/navigation-back-disposal.spec.ts`)

New test file, four scenarios, all as engineer against `http://10.77.9.231`:

1. **Mixer → landing via browser back.** Navigate to `/engineer`, wait for the WebSocket to be streaming (disconnected banner gone, at least one channel visible, status dot pulse observed), then `page.goBack()`. Assert no panic overlay, no new `/api/client-error` POST during a 3-second settling window, no `console.error` / `console.warn`.
2. **Mixer → landing via in-page back button.** Same as (1) but use `page.click("[data-testid=back-button]")` (or the equivalent selector).
3. **Mixer → different member's mixer.** Navigate to `/engineer`, wait for streaming, then navigate to `/petronela` (engineer can access any member). Same assertions — this catches the case where one disposed `MixerPage` races with its replacement.
4. **Mixer → landing → mixer loop, three iterations.** Navigate to `/engineer`, wait for streaming, back to `/`, forward to `/engineer`, repeat three times. This catches "second-mount accumulates intervals" bugs where a cleanup race means the replacement scope inherits ghost callbacks.

Each scenario attaches a `page.on("console")` listener before navigation, intercepts `/api/client-error` via `page.route`, and asserts all three oracles at the end: no overlay, no console errors/warnings, no error POST.

The 3-second settling window is chosen to cover at least two full reconnect-interval ticks (each 2 seconds) so that any leftover interval ghost will have fired and been caught.

The test runs in the existing post-deploy E2E job on the self-hosted runner. No new CI job.

## Verification plan

1. **Scanner unit tests** pass: `python3 scripts/test_check_disposal_safety.py` exits 0 with all cases green.
2. **Scanner gate** passes: `python3 scripts/check_disposal_safety.py` exits 0 against the fully-swept tree.
3. **Rust test suite** passes: full `cargo test --workspace` on CI.
4. **Build + deploy** succeeds on the self-hosted runner.
5. **E2E post-deploy** passes: all four scenarios of `navigation-back-disposal.spec.ts` green against the live deployed 1.144.0 build.
6. **Log verification**: after deploy, read `%APPDATA%\iem-mixer\logs\iem-mixer.log.<today>` via the `win-iem-snv` MCP, grep for `client_error.*disposed`, and confirm zero matches appear after the deploy timestamp. The log had 50+ matches/minute in the 1.143.0 window; after the fix the count for the equivalent window must be zero.

All six layers are required. Layers 5 and 6 are the only ones that prove the user's reported symptom is gone.

## Rollout

Single PR from `dev` to `main`. One merge commit. No feature flags. The change is purely defensive hardening of existing behavior — no user-visible functional change, no new UI, no data migration, no backwards-compatibility concerns.

## Out of scope

- Architectural restructuring (issue #165)
- Any REAPER-side change
- Any UI / visual change
- Feature work of any kind
- Revisiting the v1.142.0 panic hook or the v1.143.0 danger-zone scanner behavior (beyond the rewrite in step 3)

## Success criteria

- Panic stream at `iem_server::client_error` for `panic=Tried to access a reactive value that has already been disposed` drops from ~1 Hz to zero on the live system.
- The user can navigate mixer → landing → mixer indefinitely on the Android PWA without ever seeing the panic-hook error page.
- The CI scanner gate prevents any future plain `.set()` / `.update()` on a `set_*` identifier from being merged.
- Scanner self-tests lock in both the positive and negative rules so the scanner cannot silently degrade.
