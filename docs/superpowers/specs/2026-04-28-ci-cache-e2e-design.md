# CI Cache for E2E Job — Design

**Date:** 2026-04-28
**Status:** Approved
**Scope:** Single PR `dev` → `main`, CI-only changes (no Rust, no version bump).

## Problem

The GitHub-hosted `e2e` job (`.github/workflows/ci.yml` line 369, `runs-on: ubuntu-latest`) uses `actions/setup-node@v4` without `cache: 'npm'`, and there is no `actions/cache@v4` step for the Playwright browser cache (`~/.cache/ms-playwright`). On every push:

- `npm install` re-downloads all dependencies from npmjs.org (~10–30s wasted)
- `npx playwright install --with-deps chromium` re-downloads ~150 MB of Chromium binaries (~30–90s wasted)

Estimated waste: ~2–3 min per push on the `e2e` job. Over a typical week with 30 pushes that is ~60–90 min of unnecessary CI time and runner cost.

## Non-goals

- The self-hosted `deploy` job (`runs-on: [self-hosted, iem-lan]`, line 738) is **out of scope.** Self-hosted runners persist files between runs on local disk; adding `actions/cache@v4` would route through GitHub Actions cache (upload + download) and add overhead without saving real time.
- No version bump (this PR does not change runtime code).
- No changes to test logic, dependencies, or `npm install` semantics.
- No switch from `npm install` to `npm ci` (consistency with post-deploy job is desirable but a separate concern).

## Design

Two edits to `.github/workflows/ci.yml`, both within the `e2e` job:

### Edit 1 — Enable npm cache on `setup-node`

**Current (around line 404–408):**

```yaml
- name: Setup Node.js
  uses: actions/setup-node@v4
  with:
    node-version: "20"
```

**After:**

```yaml
- name: Setup Node.js
  uses: actions/setup-node@v4
  with:
    node-version: "20"
    cache: "npm"
    cache-dependency-path: iem-mixer/e2e/package-lock.json
```

`cache: 'npm'` caches `~/.npm` (the npm download cache, not `node_modules`). `cache-dependency-path` is required because the lockfile lives in a subdirectory.

### Edit 2 — Add Playwright browser cache

**Insert immediately BEFORE the existing `Install Playwright` step (around line 410):**

```yaml
- name: Cache Playwright browsers
  uses: actions/cache@v4
  with:
    path: ~/.cache/ms-playwright
    key: ${{ runner.os }}-playwright-${{ hashFiles('iem-mixer/e2e/package-lock.json') }}
```

The existing `Install Playwright` step is unchanged:

```yaml
- name: Install Playwright
  working-directory: iem-mixer/e2e
  run: |
    npm install
    npx playwright install --with-deps chromium
```

On cache hit, `npx playwright install --with-deps chromium` is essentially a no-op for the browser binary download — it still installs/verifies system deps but skips the ~150 MB Chromium download.

Cache key components:

- `runner.os` — guards against the (currently impossible) case where the runner OS changes
- `hashFiles('iem-mixer/e2e/package-lock.json')` — invalidates whenever any dep in the lockfile changes; on a Playwright version bump the cache key changes and a fresh entry is built

No `restore-keys` fallback. A prefix-only restore would let stale Chromium binaries linger in `~/.cache/ms-playwright` after a Playwright version bump (the install step still pulls the correct binary, but the old one stays until the cache is rewritten). A clean miss-and-rebuild on any lockfile change is simpler and cheaper.

## Verification

The change is observable purely via CI logs and run timing:

1. **First push after merge** — cache MISS expected. CI duration should match current baseline; cache uploaded at end of run.
2. **Second push** — cache HIT expected. Look for these lines in the `e2e` job logs:
   - `Cache restored successfully` (Setup Node step) for npm
   - `Cache restored from key: Linux-playwright-<hash>` (Cache Playwright browsers step)
3. **Compare total runtime** of the `e2e` job before and after a cache HIT — expect ~2–3 min reduction.

No new tests are added. The existing E2E suite continues to run on every push and proves correctness — caching is invisible to the test code itself.

## Risk

- **Low.** Cache MISS → behavior identical to today. Stale cache → `npx playwright install` re-downloads (worst case = current behavior).
- Cross-key collisions are impossible (single OS, single lockfile).
- If a Playwright version bump lands and the cache is somehow reused incorrectly, `npx playwright install --with-deps chromium` will detect the version mismatch and download the right binary.

## Rollback

Single revert — both edits are inside one job, no cross-cutting changes.
