# CI Rust Cache — Design Spec

**Date:** 2026-05-01
**Goal:** Add `Swatinem/rust-cache@v2` to all cargo-compiling CI jobs in `.github/workflows/ci.yml` to eliminate cold-compile waste on every push.

## Problem

CI workflow has 13 jobs. None use `Swatinem/rust-cache@v2`. All 6 cargo-compiling jobs use bespoke manual `actions/cache@v4` blocks. One is broken (restore-only, no save):

| Job | Cargo? | Cache state |
|-----|--------|-------------|
| test-integrity | no (grep) | n/a |
| lint | yes (clippy) | manual `actions/cache@v4` (lines 237-248), key `cargo-lint` |
| test | yes | manual `actions/cache@v4` (lines 282-293), key `cargo-test` |
| build-wasm | yes (trunk → cargo) | manual `actions/cache@v4` (lines 322-333), key `cargo-wasm`, path includes `iem-ui/target/` |
| e2e | yes (cargo build) | manual `actions/cache@v4` (lines 388-399), key `cargo-e2e` |
| mutation-test | yes (mutants + llvm-cov) | manual `actions/cache@v4` (lines 485-496), key `cargo-mutants` |
| check-version-bump | no | n/a |
| build-tauri | yes (cargo tauri build) | manual `cache/restore@v4` ONLY (lines 652-663) — **never saves, broken** |
| build-vban | no (cmake) | cmake cache (separate, OK) |
| deploy | no | n/a |

Manual caches work (5/6) but are suboptimal:

- Bespoke job-specific keys → caches don't share base layers across jobs.
- Key only hashes `Cargo.lock` — does not invalidate on rustc version bump or feature flag change → stale-cache risk.
- No `target/` pruning — caches accumulate incremental-compilation cruft over time, growing past useful size.
- `build-tauri` is fully broken — restore step with no save step → Windows cargo cache is permanently empty, cold-compiling every push (largest job, ~30 min timeout).

Net waste: ~2-5 min per push across ubuntu jobs (slow restore from bloated caches) + ~10-15 min on `build-tauri` (Windows cold compile). Compounds across PRs and fix-iterations.

## Solution

Replace all 6 manual cache blocks with `Swatinem/rust-cache@v2`:

1. `lint` — replace lines 237-248
2. `test` — replace lines 282-293
3. `build-wasm` — replace lines 322-333
4. `e2e` — replace lines 388-399
5. `mutation-test` — replace lines 485-496
6. `build-tauri` — replace broken `cache/restore@v4` block at lines 652-663

`Swatinem/rust-cache@v2` benefits over manual:

- Auto-prunes `target/` (drops examples, tests, incremental compilation cruft) → smaller caches, faster restore
- Hashes rustc version, target triple, features, env → correct invalidation
- Saves on success only → preserves prior cache on failure
- Single consistent pattern across all 6 jobs

## Pattern

Add immediately after `Install Rust` (or `dtolnay/rust-toolchain@stable`) and before any cargo invocation:

```yaml
- name: Cache cargo build
  uses: Swatinem/rust-cache@v2
  with:
    workspaces: iem-mixer
```

`workspaces: iem-mixer` is required — Cargo workspace is under `iem-mixer/`, not repo root.

## Key strategy

Default `Swatinem/rust-cache@v2` keying handles all 6 jobs:

- Auto-namespaces by job ID, runner.os, Cargo.lock hash, rustc version, target triple, and detected features
- Each job gets its own isolated cache slot
- No manual `key:` overrides needed

Different feature flags between jobs (audio, test-helpers, standalone, audio+standalone) → separate cache slots automatically. No collisions.

## Storage budget

GHA cache limit: 10GB per repo with 7-day eviction.

| Job | Est. size | OS |
|-----|-----------|-----|
| lint | ~400MB | ubuntu |
| test | ~500MB | ubuntu |
| build-wasm | ~300MB (wasm32 target) | ubuntu |
| e2e | ~500MB | ubuntu |
| mutation-test | ~600MB | ubuntu |
| build-tauri | ~1GB | windows |

Total: ~3.3GB. Well under 10GB. Eviction LRU; jobs that run on every push stay warm.

## Replaced blocks

Each of the 6 jobs has a manual `actions/cache@v4` (or `cache/restore@v4` for build-tauri) with a path list of `~/.cargo/bin/`, `~/.cargo/registry/{index,cache}/`, `~/.cargo/git/db/`, and `iem-mixer/target/` (or `iem-mixer/iem-ui/target/` for build-wasm). Each uses a job-specific key like `${{ runner.os }}-cargo-<job>-${{ hashFiles('iem-mixer/**/Cargo.lock') }}` with a `${{ runner.os }}-cargo-` restore-keys fallback.

Plan task steps capture the exact line ranges and surrounding context for each replacement.

`build-tauri` is unique: uses `cache/restore@v4` only (no matching `cache/save@v4` step) → cache never populates. rust-cache fixes this automatically (post-step saves on success).

## Verification

- **First push after merge:** cache miss in all 6 jobs (expected). Compile times unchanged. Cache populates at end of each job (rust-cache adds a post-step).
- **Second push:** cache hit in all 6 jobs. Expect 60-80% reduction in compile-time portion of each job.
- **CI logs:** `Swatinem/rust-cache@v2` prints `Cache restored` and `Cache saved` lines per job.
- **No new automated tests** — this is infrastructure-only. Verification is observed CI duration on the second push.

## Out of scope

- Cross-job shared cache via sccache (more complex, marginal benefit for our workload).
- CMake cache for `build-vban` (already has its own cache, working).
- Caching `cargo install tauri-cli` / `trunk` binaries — rust-cache handles `~/.cargo/bin/` automatically.
- Composite action `.github/actions/setup-rust/` — yagni for 6 jobs, can refactor later if more jobs added.

## File map

- `.github/workflows/ci.yml` — 6 cache-block replacements

## Version bump

Per airuleset version-bumping rule: bump 1.162.0 → 1.163.0 as first commit on dev. Update README.md changelog.

## Tasks (sequential)

1. Version bump 1.162.0 → 1.163.0 + README changelog. First commit.
2. Replace manual cache in `lint` with rust-cache.
3. Replace manual cache in `test` with rust-cache.
4. Replace manual cache in `build-wasm` with rust-cache.
5. Replace manual cache in `e2e` with rust-cache.
6. Replace manual cache in `mutation-test` with rust-cache.
7. Replace broken manual cache in `build-tauri` with rust-cache.
8. Push to dev, monitor CI green (10 jobs). Expect first run cold-compile (cache miss); cache populates.
9. Open PR dev → main. Verify mergeable + clean. Stop at green PR URL.

## Hard constraints (airuleset)

- Work on `dev`. No feature branches, no worktrees.
- Local checks only `cargo fmt --all --check` (hooks block other cargo commands).
- Single PR dev → main at end. Do NOT merge without explicit user approval.
- CI monitoring: single `sleep N && gh run view <id>` in background. No /loop, no cron.
- Per `verification-before-completion`: confirm CI green via `gh run view --json status,conclusion,jobs` before completion report.
