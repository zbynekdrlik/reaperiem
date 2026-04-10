# Mutation Testing CI Gate — Design

**Date:** 2026-04-10
**Status:** Approved for implementation

## Goal

Add `cargo-mutants` as a hard CI gate so weak tests cannot be merged unnoticed. Line coverage proves tests *executed*; mutation testing proves they *verified behavior*. The airuleset standard requires this gate; the project currently lacks it.

## Approach

Run `cargo mutants --in-diff` against the diff between the current commit and `origin/main`. Only code that changed in this PR (or dev push) gets mutated, so:

- The job is fast (no need to mutate the entire codebase).
- Existing surviving mutants in legacy code do not block PRs.
- New code is held to the higher bar from day one.

If any mutant survives (i.e., a test mutation passes the test suite), the job fails. No `continue-on-error`. No graceful skipping. Hard gate.

## Scope

**Crates covered:**

- `iem-core` — pure Rust types, parsing, config logic
- `iem-server` — backend HTTP/WebSocket handlers, REAPER proxy, poller, EQ logic

**Crates skipped:**

- `iem-ui` — Leptos WASM frontend; cargo-mutants does not handle wasm32 targets well, and the UI is exercised by the live Playwright E2E suite which provides equivalent end-to-end coverage
- `src-tauri` — thin Tauri shell that mainly forwards to the embedded server; mutating it would mostly produce equivalent mutants on glue code

**Test command mirrored from existing CI:**

The mutation job must run the same test command the existing `test` job runs, so that mutants are evaluated against the same coverage:

```
cargo test -p iem-core
cargo test -p iem-server --features audio
```

cargo-mutants is invoked once with both packages selected and the `audio` feature enabled, so a single run covers both crates.

## Trigger

The new `mutation-test` job runs on:

- Every push to `dev`
- Every PR targeting `main`

It does **not** run on push to `main` itself (the diff against `main` would be empty by definition; plus all main pushes come from PR merges that already passed the gate).

## Job Topology

```
test-integrity ─┐
lint ──────────┼─→ test            ─┐
               └─→ build-wasm      ─┤
               └─→ mutation-test  ─┤  (NEW — parallel with test, gated on lint)
                                    ├─→ e2e ─→ build-tauri ─→ deploy
                                    └─ check-version-bump (PR only)
```

The `mutation-test` job depends on `lint` (so it doesn't waste cycles on broken formatting) and runs in parallel with `test`, `build-wasm`, and `e2e`. It does not block downstream build/deploy jobs unless it fails — once it fails, the workflow run is red and the deploy job will not run.

## Tool Installation

Use `taiki-e/install-action@v2` to install `cargo-mutants` as a prebuilt binary. This is roughly 20× faster than `cargo install cargo-mutants` and avoids compiling the tool on every CI run. The action handles caching automatically.

## Diff Computation

For both `dev` push and PR runs, the comparison base is `origin/main`. The job runs:

```bash
git fetch origin main --depth=200
git diff origin/main...HEAD > pr.diff
```

The triple-dot (`...`) form gives the diff from the merge base, which is the correct semantic for "what did this branch change relative to main."

If `pr.diff` is empty or contains no Rust changes in the targeted crates, `cargo mutants --in-diff` will exit cleanly with zero mutants generated. The job logs the count explicitly so a green run is auditable.

## Failure Modes

| Condition | Outcome |
|-----------|---------|
| No Rust changes in `iem-core`/`iem-server` | Job passes — explicitly logs "0 mutants generated" |
| All mutants killed by tests | Job passes — logs killed/timed-out counts |
| At least one mutant survives | Job fails with non-zero exit code — `cargo-mutants` prints surviving mutants |
| `cargo-mutants` install fails | Job fails (no fallback) |
| `git fetch` fails | Job fails (no fallback) |

There is no `continue-on-error` and no skip path. If the tool reports a problem, the workflow run is red.

## Performance

`--in-diff` only mutates lines touched by the PR. For a typical small PR (5-50 lines of Rust), expect:

- Tool install: ~10 seconds (cached binary)
- Compile baseline: 2-5 minutes (with cargo cache)
- Mutation runs: 10 seconds × N mutants, parallelized with `--jobs 4`
- Total: usually under 10 minutes for normal PRs

For large PRs touching hundreds of Rust lines, the job may take 15-20 minutes. This is acceptable — large PRs warrant deeper testing.

The job uses the same `~/.cargo` cache pattern as the `test` job (key: `${{ runner.os }}-cargo-mutants-${{ hashFiles('iem-mixer/**/Cargo.lock') }}`).

## Edge Cases Considered

1. **Test integrity scan**: The existing `test-integrity` job greps for skip patterns. cargo-mutants config does not introduce any matching patterns, so no integration concern.

2. **`integration` feature**: `iem-server` has an `integration` feature gating `tests/reaper_live.rs` (requires live REAPER). cargo-mutants will NOT enable this feature — it would fail in CI without REAPER. The default test command (`--features audio`) is correct.

3. **Workspace root vs crate root**: cargo-mutants must run from the workspace root (`iem-mixer/`). Per-crate `--package` flags select what to mutate.

4. **Timeout**: Set `--timeout 120` (per-mutant test timeout, in seconds) so a stuck test doesn't hang the job indefinitely. Default is `auto` (5× baseline), which is usually fine, but explicit is safer for CI.

5. **Cache invalidation**: When `Cargo.lock` changes, the cache is regenerated. This is acceptable and matches the existing `test` job.

## Files Modified

- `.github/workflows/ci.yml` — add `mutation-test` job
- `README.md` — changelog entry for v1.139.0 noting the new gate

## Out of Scope

- Mutation testing for `iem-ui` (WASM/Leptos)
- Mutation testing for TypeScript E2E tests (Playwright code)
- Backfilling mutation tests on legacy code (use `--in-diff` to only gate new code)
- Mutation score thresholds — `cargo-mutants` is binary (any survivor = fail), no percentage metric
- Mutation testing of Lua ReaScripts (no tooling exists)

## Verification

After deploy, verify the gate works by:

1. Confirming the new job appears in the next CI run on dev
2. Confirming it passes with the version bump commit (no Rust changes → 0 mutants → green)
3. The next code change to `iem-core`/`iem-server` will exercise the gate naturally
