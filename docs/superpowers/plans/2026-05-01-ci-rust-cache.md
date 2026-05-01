# CI Rust Cache Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace 6 manual `actions/cache@v4` blocks with `Swatinem/rust-cache@v2` in `.github/workflows/ci.yml` to eliminate cache bloat, fix the broken build-tauri cache (restore-only, never saves), and standardize on a single, correctly-keyed cache pattern across all cargo-compiling jobs.

**Architecture:** Pure CI configuration change. No application code touched. Each replacement is a localized YAML edit: delete the existing manual cache step block, insert a 4-line `Swatinem/rust-cache@v2` step at the same position. Build-tauri additionally gains a working save step (rust-cache handles save-on-success automatically via post-step).

**Tech Stack:** GitHub Actions, `Swatinem/rust-cache@v2`

**Spec:** `docs/superpowers/specs/2026-05-01-ci-rust-cache-design.md`

---

## Context

CI workflow at `.github/workflows/ci.yml` has 13 jobs. 6 cargo-compiling jobs (lint, test, build-wasm, e2e, mutation-test, build-tauri) currently use bespoke manual `actions/cache@v4` blocks with job-specific keys hashing only `Cargo.lock`. `build-tauri` uses `cache/restore@v4` ONLY with no matching `cache/save@v4` step — its Windows cargo cache permanently empty, cold-compiling every push (largest job, ~30 min timeout).

This plan replaces all 6 with a single `Swatinem/rust-cache@v2` step per job. Net benefit: smaller caches (auto-pruned `target/`), correct invalidation (hashes rustc + features + target triple, not just Cargo.lock), automatic save-on-success, and a consistent pattern.

**Critical project facts:**

- Repo at `/home/newlevel/devel/reaperiem`. Cargo workspace at `iem-mixer/Cargo.toml`. Plan refers to `workspaces: iem-mixer` for rust-cache.
- Two branches only: `dev` (work here) and `main` (PR target). No feature branches.
- Git hooks block `cargo build/test/clippy/check` locally. Only `cargo fmt --all --check` allowed.
- CI monitoring: single `sleep N && gh run view <id> --json status,conclusion,jobs` in background. NO `/loop`, NO cron, NO custom monitor scripts.
- Project uses `dtolnay/rust-toolchain@stable` for toolchain installation. rust-cache step goes immediately after toolchain install, before any cargo invocation.

---

## File Map

### Code change

- `.github/workflows/ci.yml` — 6 cache-block replacements (one per job)

### Version bump (per airuleset)

- `iem-mixer/crates/iem-core/Cargo.toml`
- `iem-mixer/Cargo.toml`
- `iem-mixer/crates/iem-server/Cargo.toml`
- `iem-mixer/iem-ui/Cargo.toml`
- `iem-mixer/src-tauri/Cargo.toml`
- `iem-mixer/src-tauri/tauri.conf.json`
- `README.md` (changelog entry)

---

## Task 1: Version Bump (1.162.0 → 1.163.0) + Changelog

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`
- Modify: `README.md`

- [ ] **Step 1: Bump all version files**

```bash
sed -i 's/version = "1.162.0"/version = "1.163.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.162.0"/"version": "1.163.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify bump landed**

```bash
grep -c '1.163.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Expected: each returns 1
```

- [ ] **Step 3: Insert changelog entry into README.md**

Find the line `## Changelog` in `README.md` and insert this entry as the new top entry (immediately under the `## Changelog` heading and any existing top entry — newest entries first):

```markdown
### v1.163.0 (2026-05-01)

- **CI**: Replace 6 manual `actions/cache@v4` blocks with `Swatinem/rust-cache@v2`. Fixes broken `build-tauri` cache (was restore-only, never saved). Standardizes on auto-pruned target dirs and correctly-keyed cache (rustc version + Cargo.lock + features). Cuts ~10-15 min from `build-tauri` after warm-up; ~2-5 min from ubuntu jobs.
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json \
  README.md
git commit -m "chore: bump version to 1.163.0"
```

---

## Task 2: Replace lint job's manual cache

**File:** `.github/workflows/ci.yml`, lines 237-248

The `lint` job currently has a manual `actions/cache@v4` block between `Install Rust` (line 232) and `Check formatting` (line 250). Replace it with `Swatinem/rust-cache@v2`.

- [ ] **Step 1: Read current lint cache block**

Read lines 237-248 of `.github/workflows/ci.yml` to confirm exact current state:

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/target/
          key: ${{ runner.os }}-cargo-lint-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

- [ ] **Step 2: Replace with rust-cache**

Use the Edit tool to replace the block above with:

```yaml
      - name: Cache cargo build
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: iem-mixer
```

The `old_string` for the Edit must include the leading newline + `      - name: Cache cargo` so it is unique within the file (other jobs have similarly-named steps but with different keys).

- [ ] **Step 3: Verify**

```bash
grep -n 'Swatinem/rust-cache@v2' .github/workflows/ci.yml
# Expected: at least 1 line in the lint job's range (~237-241)
```

Confirm no leftover `actions/cache@v4` text remains in lint section by reading lines 237-260.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(lint): replace manual actions/cache@v4 with Swatinem/rust-cache@v2"
```

---

## Task 3: Replace test job's manual cache

**File:** `.github/workflows/ci.yml`, lines 282-293

The `test` job has the same pattern as lint, with key `cargo-test`.

- [ ] **Step 1: Read current test cache block**

Confirm the block at lines 282-293:

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/target/
          key: ${{ runner.os }}-cargo-test-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

- [ ] **Step 2: Replace with rust-cache**

Use the Edit tool. To make `old_string` unique, include enough context: the exact key `cargo-test` distinguishes this from other jobs. Example unique `old_string`:

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/target/
          key: ${{ runner.os }}-cargo-test-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

`new_string`:

```yaml
      - name: Cache cargo build
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: iem-mixer
```

- [ ] **Step 3: Verify**

```bash
grep -c 'Swatinem/rust-cache@v2' .github/workflows/ci.yml
# Expected: 2 (lint + test)
grep -c 'cargo-test-${{' .github/workflows/ci.yml
# Expected: 0 (old key removed)
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(test): replace manual actions/cache@v4 with Swatinem/rust-cache@v2"
```

---

## Task 4: Replace build-wasm job's manual cache

**File:** `.github/workflows/ci.yml`, lines 322-333

`build-wasm` uses `iem-mixer/iem-ui/target/` (NOT `iem-mixer/target/`) and hashes `iem-mixer/iem-ui/Cargo.lock`. rust-cache auto-detects this via the `workspaces` parameter — point it at the iem-ui sub-workspace.

- [ ] **Step 1: Read current build-wasm cache block**

Confirm at lines 322-333:

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/iem-ui/target/
          key: ${{ runner.os }}-cargo-wasm-${{ hashFiles('iem-mixer/iem-ui/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

- [ ] **Step 2: Replace with rust-cache pointing at iem-ui**

`old_string` (the block above is already unique due to the iem-ui-specific path and `cargo-wasm` key):

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/iem-ui/target/
          key: ${{ runner.os }}-cargo-wasm-${{ hashFiles('iem-mixer/iem-ui/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

`new_string`:

```yaml
      - name: Cache cargo build
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: iem-mixer/iem-ui
```

Note: `workspaces: iem-mixer/iem-ui` — `iem-ui` has its own `Cargo.lock` and `target/` separate from the parent workspace. rust-cache reads the `Cargo.toml`/`Cargo.lock` at that path.

- [ ] **Step 3: Verify**

```bash
grep -c 'Swatinem/rust-cache@v2' .github/workflows/ci.yml
# Expected: 3
grep -c 'workspaces: iem-mixer/iem-ui' .github/workflows/ci.yml
# Expected: 1
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(build-wasm): replace manual actions/cache@v4 with Swatinem/rust-cache@v2"
```

---

## Task 5: Replace e2e job's manual cache

**File:** `.github/workflows/ci.yml`, lines 388-399

The `e2e` job builds `iem-server` with `standalone,audio` features. Same workspace as lint/test (`iem-mixer/Cargo.lock`).

- [ ] **Step 1: Read current e2e cache block**

Confirm at lines 388-399:

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/target/
          key: ${{ runner.os }}-cargo-e2e-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

- [ ] **Step 2: Replace with rust-cache**

`old_string`:

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/target/
          key: ${{ runner.os }}-cargo-e2e-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

`new_string`:

```yaml
      - name: Cache cargo build
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: iem-mixer
```

- [ ] **Step 3: Verify**

```bash
grep -c 'Swatinem/rust-cache@v2' .github/workflows/ci.yml
# Expected: 4
grep -c 'cargo-e2e-${{' .github/workflows/ci.yml
# Expected: 0
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(e2e): replace manual actions/cache@v4 with Swatinem/rust-cache@v2"
```

---

## Task 6: Replace mutation-test job's manual cache

**File:** `.github/workflows/ci.yml`, lines 485-496

`mutation-test` job has the manual cache between `Install Rust` (line 482-483) and `Install cargo-mutants` (line 498). Same workspace.

- [ ] **Step 1: Read current mutation-test cache block**

Confirm at lines 485-496:

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/target/
          key: ${{ runner.os }}-cargo-mutants-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

- [ ] **Step 2: Replace with rust-cache**

`old_string`:

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/target/
          key: ${{ runner.os }}-cargo-mutants-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

`new_string`:

```yaml
      - name: Cache cargo build
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: iem-mixer
```

The subsequent steps (`Install cargo-mutants`, `Install cargo-llvm-cov`) remain unchanged. Tool binaries get cached automatically via rust-cache's `~/.cargo/bin/` handling.

- [ ] **Step 3: Verify**

```bash
grep -c 'Swatinem/rust-cache@v2' .github/workflows/ci.yml
# Expected: 5
grep -c 'cargo-mutants-${{' .github/workflows/ci.yml
# Expected: 0
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(mutation-test): replace manual actions/cache@v4 with Swatinem/rust-cache@v2"
```

---

## Task 7: Replace broken build-tauri restore-only cache

**File:** `.github/workflows/ci.yml`, lines 652-663

`build-tauri` (Windows runner) uses `actions/cache/restore@v4` ONLY — no matching `actions/cache/save@v4` step. Cache permanently empty. rust-cache replaces both halves with one step that does restore-on-entry and save-on-exit (post-step).

- [ ] **Step 1: Read current build-tauri cache block**

Confirm at lines 652-663:

```yaml
      - name: Restore cargo cache
        uses: actions/cache/restore@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/target/
          key: ${{ runner.os }}-cargo-tauri-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

Note the unique step name `Restore cargo cache` (vs `Cache cargo` in other jobs) and the use of `cache/restore@v4` action.

- [ ] **Step 2: Replace with rust-cache**

`old_string`:

```yaml
      - name: Restore cargo cache
        uses: actions/cache/restore@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            iem-mixer/target/
          key: ${{ runner.os }}-cargo-tauri-${{ hashFiles('iem-mixer/**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

`new_string`:

```yaml
      - name: Cache cargo build
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: iem-mixer
```

- [ ] **Step 3: Verify all 6 replacements landed**

```bash
grep -c 'Swatinem/rust-cache@v2' .github/workflows/ci.yml
# Expected: 6
grep -cE 'actions/cache(@v4|/restore@v4)' .github/workflows/ci.yml
# Expected: 1 (only the build-vban CMake cache remains, which is correct and out of scope)
grep -c 'cargo-tauri-${{' .github/workflows/ci.yml
# Expected: 0
grep -c 'cache/restore@v4' .github/workflows/ci.yml
# Expected: 0 (the broken restore-only block is gone)
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(build-tauri): replace broken restore-only cache with Swatinem/rust-cache@v2

The previous setup used actions/cache/restore@v4 with no matching save
step, so the Windows cargo cache was permanently empty and build-tauri
cold-compiled every push. rust-cache handles both restore (entry) and
save (post-step on success) in a single action."
```

---

## Task 8: Push to dev and monitor CI

**No file changes in this task.** Push the 7 commits accumulated above (1 version bump + 6 cache replacements) and watch CI.

- [ ] **Step 1: Run local fmt check**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all --check
cd /home/newlevel/devel/reaperiem
```

Expected: no output (clean). Local-only — hooks block other cargo commands.

- [ ] **Step 2: Push dev**

```bash
git push origin dev
```

Capture the run ID:

```bash
gh run list --branch dev --limit 1 --json databaseId,status,conclusion,headSha
```

- [ ] **Step 3: Monitor CI in background**

```bash
RUN_ID=$(gh run list --branch dev --limit 1 --json databaseId --jq '.[0].databaseId')
echo "Monitoring run $RUN_ID"
```

Single background command per airuleset:

```bash
sleep 300 && gh run view $RUN_ID --json status,conclusion,jobs
```

Run via Bash with `run_in_background: true`. Wait for the notification — DO NOT poll, DO NOT use `/loop`, DO NOT spawn custom monitor scripts.

- [ ] **Step 4: React to CI result**

When the background command completes:

- All jobs `success` → proceed to Task 9.
- Any job `failure` → run `gh run view $RUN_ID --log-failed` and investigate. **DO NOT blindly rerun.** Common expected outcomes for THIS PR specifically:
  - Cache miss in all 6 jobs (FIRST run after merge to a branch is always a miss). This is expected — compile times unchanged on first run. Cache populates on success.
  - YAML parse error → fix indentation in the offending block, push fix, re-monitor.
  - `Swatinem/rust-cache@v2` action not found → typo in `uses:` line, correct it, push fix.
  - Failure unrelated to cache change (e.g., flaky e2e test) → investigate that specific failure on its merits per `ci-monitoring.md`.

If a fix is needed, all corrective changes go in ONE commit, then ONE push. Re-monitor via the same single background `sleep 300 && gh run view` pattern.

- [ ] **Step 5: Confirm cache populated**

After CI is green, inspect at least one job's logs to confirm `Swatinem/rust-cache@v2` ran and reported a save:

```bash
gh run view $RUN_ID --log --job=<job-id> | grep -E 'Cache (restored|saved|miss)'
```

Expected on first run: `Cache miss` then `Saving cache` (post-step).

---

## Task 9: Open PR dev → main and STOP

- [ ] **Step 1: Confirm dev is ahead of main and up-to-date with origin/main**

```bash
git fetch origin
git log --oneline origin/main..origin/dev | head -10
# Should show the 7 commits from Tasks 1-7
git log --oneline origin/dev..origin/main
# Should be empty (no commits on main that dev lacks)
```

If dev is behind main, sync: `git merge origin/main && git push origin dev`. Re-monitor CI.

- [ ] **Step 2: Create PR**

```bash
gh pr create --base main --head dev --title "ci: replace manual cargo caches with Swatinem/rust-cache@v2" --body "$(cat <<'EOF'
## Summary

- Replace 6 manual `actions/cache@v4` cargo cache blocks with `Swatinem/rust-cache@v2`
- Fix broken `build-tauri` cache (was `actions/cache/restore@v4` with no matching save step → permanently empty)
- Standardize on a single, correctly-keyed cache pattern across all cargo-compiling jobs

## Why

The existing manual caches:
- Use job-specific keys hashing only `Cargo.lock` → don't invalidate on rustc bumps or feature flag changes (stale-cache risk)
- Skip `target/` pruning → caches accumulate incremental-compilation cruft over time
- `build-tauri` is fully broken — restore-only, never saves → Windows cargo cold-compiles every push

`Swatinem/rust-cache@v2` is the standard solution: auto-pruned target dirs, correct key derivation (rustc + Cargo.lock + features + target triple + env), automatic save-on-success.

## Expected impact

- `build-tauri` (Windows): biggest win, was cold-compiling every push (~30 min timeout). Now warm after first run.
- Ubuntu jobs (lint, test, build-wasm, e2e, mutation-test): smaller savings (~2-5 min each from leaner caches and faster restores).
- First push after merge is a cache miss in all 6 jobs (expected, then warm).

## Test plan

- [x] All 6 manual cache blocks replaced with `Swatinem/rust-cache@v2`
- [x] No leftover `actions/cache@v4` or `cache/restore@v4` blocks for cargo
- [x] CI green on dev
- [ ] Second push to a PR shows cache hits (verify after merge)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Verify PR is mergeable and clean**

```bash
PR_NUMBER=$(gh pr list --head dev --base main --limit 1 --json number --jq '.[0].number')
gh api repos/zbynekdrlik/reaperiem/pulls/$PR_NUMBER --jq '{mergeable, mergeable_state}'
```

Expected: `{"mergeable": true, "mergeable_state": "clean"}`.

If `mergeable_state` is not `clean`:
- `behind` → sync dev with main: `git fetch origin && git merge origin/main && git push origin dev`. Re-monitor CI.
- `blocked` → some required check failed or is missing. Investigate and fix.
- `dirty` → merge conflict. Resolve on dev.
- `unstable` → check failed or is non-blocking-but-failing. Investigate per `autonomous-quality-discipline.md`. NEVER admin-merge.

- [ ] **Step 4: STOP at green PR URL**

Get the PR URL:

```bash
gh pr view $PR_NUMBER --json url --jq .url
```

Send the completion report (per airuleset `completion-report.md`) and **STOP**. Do NOT merge without explicit user approval.

---

## Task Dependencies

```
Task 1 (version bump + changelog)  ─┐
                                    ▼
Task 2 (lint cache)                ─┐
Task 3 (test cache)                ─┤
Task 4 (build-wasm cache)          ─┤── independent yet sequential per ask-before-assuming pattern
Task 5 (e2e cache)                 ─┤   (one PR, one push, no parallel branches)
Task 6 (mutation-test cache)       ─┤
Task 7 (build-tauri cache)         ─┘
                                    │
                                    ▼
Task 8 (push + CI monitor)
                                    │
                                    ▼
Task 9 (PR creation + STOP)
```

Tasks 2-7 each touch the same file (`ci.yml`) but at distinct, non-overlapping line ranges. Sequential execution avoids merge churn. Each task ends with its own commit so reverting any individual job's cache change is trivial.

---

## Verification

After CI is green and PR is mergeable + clean:

1. **All 10 active CI jobs pass** (test-integrity, lint, test, build-wasm, e2e, mutation-test, build-tauri, build-vban, deploy, plus check-version-bump on PR event).
2. **CI logs show `Swatinem/rust-cache@v2` ran** in each of the 6 cargo-compiling jobs. First push = cache miss (expected). Cache populates on success via post-step.
3. **No leftover manual cargo caches** — `grep -c 'actions/cache.*cargo' .github/workflows/ci.yml` returns 0; the only `actions/cache@v4` left should be the CMake cache in `build-vban` (out of scope).
4. **PR mergeable + clean** — `gh api ... --jq '.mergeable_state'` returns `"clean"`.
5. **STOP at green PR URL.** No merge without explicit user approval.

After merge to main, the SECOND push to a PR (or to dev) should show `Cache restored from key: ...` for each of the 6 jobs and a measurable reduction in compile-step duration vs. the historical baseline.
