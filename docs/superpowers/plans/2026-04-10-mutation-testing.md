# Mutation Testing CI Gate — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `cargo-mutants` as a hard CI gate so weak tests cannot ship unnoticed. Uses `--in-diff` against `origin/main` so only newly-changed code is gated.

**Architecture:** New `mutation-test` job in `.github/workflows/ci.yml`, parallel with `test`/`build-wasm`/`e2e`, depending on `lint`. Mutates `iem-core` and `iem-server` only. Hard fail on any surviving mutant.

**Tech Stack:** GitHub Actions, cargo-mutants, taiki-e/install-action

**Spec:** `docs/superpowers/specs/2026-04-10-mutation-testing-design.md`

---

## Context

The version bump to 1.139.0 has already been committed (`8afada6`). The spec has been committed (`aa76d1b`). This plan only adds the new CI job and the changelog entry.

**Critical facts:**
- The existing `test` job runs `cargo test -p iem-core` and `cargo test -p iem-server --features audio` from `iem-mixer/`
- `iem-server` has features: `default = []`, `audio`, `tls`, `standalone`, `integration`. cargo-mutants must use `--features audio` (matches test job) and must NOT enable `integration` (requires live REAPER)
- `iem-core` has `default = ["config"]` — no special features needed
- The runner is `ubuntu-latest`
- The workspace root is `iem-mixer/`
- Existing cargo cache key pattern: `${{ runner.os }}-cargo-<purpose>-${{ hashFiles('iem-mixer/**/Cargo.lock') }}`
- The test-integrity job already enforces no `continue-on-error` — adding one would fail CI

---

## File Map

### Modified files
- `.github/workflows/ci.yml` — add `mutation-test:` job after the `e2e:` job (around line 388, before `check-version-bump:`)
- `README.md` — add changelog entry for v1.139.0

### No new files
The plan creates no new files — everything goes into existing CI workflow and changelog.

---

## Task 1: Add `mutation-test` job to ci.yml

**File:** `.github/workflows/ci.yml`

Add the new job after the `e2e:` job's last line and before the `check-version-bump:` job. The exact insertion point is after the closing of the `e2e:` job (before line 389 which currently reads `  check-version-bump:`).

- [ ] **Step 1: Insert the new job**

Find the line `  check-version-bump:` and insert the following block immediately ABOVE it (with one blank line of separation):

```yaml
  # ============================================================
  # MUTATION TESTING - Test quality gate (kills weak tests)
  # ============================================================
  mutation-test:
    name: Mutation Testing
    runs-on: ubuntu-latest
    timeout-minutes: 25
    needs: lint
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # need full history for diff against origin/main

      - name: Install system dependencies
        run: sudo apt-get update && sudo apt-get install -y libopus-dev pkg-config

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

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

      - name: Install cargo-mutants
        uses: taiki-e/install-action@v2
        with:
          tool: cargo-mutants

      - name: Create minimal dist folder for rust-embed
        run: |
          mkdir -p iem-mixer/iem-ui/dist
          echo "placeholder" > iem-mixer/iem-ui/dist/index.html

      - name: Compute diff against origin/main
        working-directory: iem-mixer
        run: |
          git fetch origin main --depth=200
          # Triple-dot diff: changes from merge base to HEAD
          git diff origin/main...HEAD -- 'crates/iem-core/**/*.rs' 'crates/iem-server/**/*.rs' > pr.diff
          echo "Diff size: $(wc -l < pr.diff) lines"
          if [ ! -s pr.diff ]; then
            echo "No Rust changes in iem-core or iem-server — nothing to mutate"
          fi

      - name: Run mutation testing
        working-directory: iem-mixer
        run: |
          if [ ! -s pr.diff ]; then
            echo "Skipping cargo-mutants run: empty diff"
            exit 0
          fi
          cargo mutants \
            --in-diff pr.diff \
            --package iem-core \
            --package iem-server \
            --features iem-server/audio \
            --timeout 120 \
            --jobs 4 \
            --no-shuffle

```

- [ ] **Step 2: Verify the YAML parses**

Run a syntax check by listing the workflow with GitHub's parser locally if available, or simply confirm the indentation matches the surrounding jobs (4 spaces for `name:`/`runs-on:`/`needs:`/`steps:` keys, 6 spaces for step entries). The block above is already correctly indented to match `e2e` and `check-version-bump`.

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML OK"
```

Expected: `YAML OK`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add cargo-mutants test quality gate (--in-diff)"
```

---

## Task 2: Add changelog entry to README.md

**File:** `README.md`

The changelog section uses the format documented in `CLAUDE.md`. Find the `## Changelog` section and add a new entry at the top of the version list.

- [ ] **Step 1: Locate the changelog**

```bash
grep -n "^## Changelog" README.md
grep -n "^### v1.138" README.md
```

The new entry goes immediately after `## Changelog` and before the `### v1.138.0` line.

- [ ] **Step 2: Insert the v1.139.0 entry**

Add this block immediately above the existing `### v1.138.0` heading:

```markdown
### v1.139.0 (2026-04-10)

- **CI**: Added `cargo-mutants` test quality gate. Mutation testing runs on every dev push and PR, mutating only code changed vs `origin/main` (`--in-diff`). Any surviving mutant fails CI. Covers `iem-core` and `iem-server`. Catches weak tests that exercise code without verifying behavior.

```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: changelog entry for v1.139.0 (mutation testing gate)"
```

---

## Task 3: Push and monitor CI

- [ ] **Step 1: Push to dev**

```bash
git push origin dev
```

- [ ] **Step 2: Monitor the run until ALL jobs reach a terminal state**

```bash
gh run list --branch dev --limit 3
# Identify the latest run id triggered by this push, then:
gh run view <run-id>
```

Wait until every job (including the new `mutation-test`, plus deploy) is either ✅ success or ❌ failed. Do not declare success while anything is still running.

- [ ] **Step 3: Verify the new job behavior**

The new `mutation-test` job is expected to PASS on this run because the only Rust changes in this branch are version-bump lines in `Cargo.toml` (not `.rs` files), so the diff filter `crates/iem-core/**/*.rs` and `crates/iem-server/**/*.rs` will produce an empty `pr.diff` and the job will skip cleanly with "0 mutants".

Confirm the job logs show:
- `Diff size: 0 lines`
- `Skipping cargo-mutants run: empty diff`
- Step exits with 0

- [ ] **Step 4: If CI fails, investigate with `gh run view <id> --log-failed` and fix all issues in ONE commit**

Common expected issues:
- YAML indentation mismatch → fix and recommit
- `cargo-mutants` install action version change → check `taiki-e/install-action` for current syntax
- `git fetch` failing on shallow clone → already mitigated with `fetch-depth: 0`
- Feature flag syntax: cargo-mutants uses `--features <crate>/<feature>` for workspace selections; if rejected, fall back to `--features audio` and rely on the workspace default

---

## Task 4: Create PR from dev to main

- [ ] **Step 1: Verify CI green on dev**

```bash
gh run list --branch dev --limit 3
```

All jobs must be ✅ before opening the PR.

- [ ] **Step 2: Create the PR**

```bash
gh pr create --base main --head dev \
  --title "ci: add cargo-mutants test quality gate" \
  --body "$(cat <<'EOF'
## Summary
- Adds `cargo-mutants` as a hard CI gate covering `iem-core` and `iem-server`
- Uses `--in-diff origin/main...HEAD` so only newly-changed code is mutated (legacy debt is not blocking)
- Hard fail on any surviving mutant — no skip path, no `continue-on-error`
- Bumps version to 1.139.0

## Test plan
- [x] YAML parses cleanly
- [x] CI green on dev (mutation-test job logs "0 mutants" on the version-bump commit since no .rs files changed)
- [ ] Next code change to iem-core or iem-server will exercise the gate end-to-end

Spec: docs/superpowers/specs/2026-04-10-mutation-testing-design.md
Plan: docs/superpowers/plans/2026-04-10-mutation-testing.md
EOF
)"
```

- [ ] **Step 3: Verify the PR is mergeable**

```bash
gh pr view <pr-number> --json mergeable,mergeStateStatus
```

Expected: `mergeable: MERGEABLE` and `mergeStateStatus: CLEAN`. If "behind" or "blocked", investigate before reporting to the user.

- [ ] **Step 4: Wait for explicit user merge approval**

Per the user's standing rule, do NOT merge the PR. Report the green PR URL and wait for explicit "merge it" / "approved".

---

## Task Dependencies

```
Task 1 (add CI job)
   │
   ▼
Task 2 (changelog)
   │
   ▼
Task 3 (push + monitor)
   │
   ▼
Task 4 (open PR + wait for approval)
```

These are sequential — each task's commit feeds into the next.

---

## Verification

After CI is green and the PR is open:

1. **All 10 CI jobs** pass on dev (the existing 9 + the new `mutation-test`)
2. **Deploy** to iem.lan succeeds and verification passes
3. **PR** is mergeable, clean, all checks green
4. **Spec preserved**: The new gate is documented in `docs/superpowers/specs/`
5. Wait for the user's explicit merge command before merging
