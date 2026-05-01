# CI Rust Cache — Design Spec

**Date:** 2026-05-01
**Goal:** Add `Swatinem/rust-cache@v2` to all cargo-compiling CI jobs in `.github/workflows/ci.yml` to eliminate cold-compile waste on every push.

## Problem

CI workflow has 13 jobs. None use `Swatinem/rust-cache@v2`. Cargo workspace at `iem-mixer/Cargo.toml` cold-compiles every push. Two jobs have manual cache attempts, one of which is broken:

| Job | Cargo? | Cache state |
|-----|--------|-------------|
| test-integrity | no (grep) | n/a |
| lint | yes (clippy) | NONE |
| test | yes | NONE |
| build-wasm | yes (trunk → cargo) | NONE |
| e2e | yes (cargo build) | NONE |
| mutation-test | yes (mutants + llvm-cov) | manual `actions/cache@v4` (works, bespoke key) |
| check-version-bump | no | n/a |
| build-tauri | yes (cargo tauri build) | manual `cache/restore@v4` ONLY — **never saves, broken** |
| build-vban | no (cmake) | cmake cache (separate, OK) |
| deploy | no | n/a |

Estimated waste: 8-15 min per push across the 6 cargo jobs. Compounds across PRs and fix-iterations.

## Solution

Add `Swatinem/rust-cache@v2` to 6 cargo-compiling jobs:

1. `lint`
2. `test`
3. `build-wasm`
4. `e2e`
5. `mutation-test` (**replace** manual `actions/cache@v4` block at lines 485-496)
6. `build-tauri` (**replace** broken manual `cache/restore@v4` block at lines 652-663)

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

### mutation-test (lines 485-496) — DELETE

```yaml
- name: Cache cargo
  uses: actions/cache@v4
  with:
    path:
      ~/.cargo/bin/
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      iem-mixer/target/
    key: ${{ runner.os }}-cargo-mutants-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
    restore-keys: |
      ${{ runner.os }}-cargo-
```

Replace with the standard rust-cache step.

### build-tauri (lines 652-663) — DELETE

```yaml
- name: Restore cargo cache
  uses: actions/cache/restore@v4
  with:
    path:
      ~/.cargo/bin/
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      iem-mixer/target/
    key: ${{ runner.os }}-cargo-tauri-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
    restore-keys: |
      ${{ runner.os }}-cargo-
```

Note: this uses `cache/restore@v4` only — there is no matching `cache/save@v4` step in the job, so this cache never populates. Bug. rust-cache fixes it (does both restore on entry and save on completion via post-step).

Replace with the standard rust-cache step.

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

- `.github/workflows/ci.yml` — 6 edits (4 inserts, 2 replaces)

## Version bump

Per airuleset version-bumping rule: bump 1.162.0 → 1.163.0 as first commit on dev. Update README.md changelog.

## Tasks (sequential)

1. Version bump 1.162.0 → 1.163.0 + README changelog. First commit.
2. Add rust-cache to `lint`.
3. Add rust-cache to `test`.
4. Add rust-cache to `build-wasm`.
5. Add rust-cache to `e2e`.
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
