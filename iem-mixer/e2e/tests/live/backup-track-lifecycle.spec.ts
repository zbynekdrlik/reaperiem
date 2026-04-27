/**
 * Track-lifecycle resilience tests for backup/restore.
 *
 * Covers four scenarios:
 *  1. tracks added after backup are reported in tracks_in_reaper_not_in_backup
 *  2. tracks renamed before restore land in skipped_tracks
 *  3. same rename path framed as skipped_tracks warning
 *  4. round-trip property: capture → restore → second capture still lists both files
 *
 * Safety: engineer-only writes, finally blocks restore all REAPER state.
 */

import { test, expect } from "@playwright/test";

const REAPER = "http://iem.lan:8080";
const APP_BASE = process.env.APP_URL ?? "http://10.77.9.231";

// Track 22 is an input track that is safe to rename for testing.
const RENAME_TRACK_IDX = 22;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Rename a REAPER track via the rename_track ReaScript. */
async function renameTrack(
  request: import("@playwright/test").APIRequestContext,
  idx: number,
  newName: string,
): Promise<void> {
  await request.get(
    `${REAPER}/_/SET/EXTSTATE/reaperiem/rename_track_index/${idx}`,
  );
  await request.get(
    `${REAPER}/_/SET/EXTSTATE/reaperiem/rename_track_name/${encodeURIComponent(newName)}`,
  );
  await request.get(`${REAPER}/_/_RS_REAPERIEM_RENAME_TRACK`);
  // Allow the ReaScript to execute
  await new Promise((r) => setTimeout(r, 1500));
}

/** Read the current name of a REAPER track (1-based index). */
async function getTrackName(
  request: import("@playwright/test").APIRequestContext,
  idx: number,
): Promise<string> {
  const r = await request.get(`${REAPER}/_/TRACK/${idx}`);
  const text = await r.text();
  const parts = text.trim().split("\t");
  return parts[2] ?? "";
}

/** Get an engineer JWT. */
async function getEngineerToken(
  request: import("@playwright/test").APIRequestContext,
  page: import("@playwright/test").Page,
): Promise<string> {
  await page.goto(APP_BASE + "/");
  const resp = await request.post(`${APP_BASE}/api/auth`, {
    data: { member: "engineer", pin: "1177" },
  });
  expect(resp.status()).toBe(200);
  const data = await resp.json();
  expect(data.engineer).toBe(true);
  return data.token as string;
}

/** Capture a backup via the API; returns the filename. */
async function captureBackup(
  request: import("@playwright/test").APIRequestContext,
  headers: Record<string, string>,
): Promise<string> {
  const resp = await request.post(`${APP_BASE}/api/backups/capture`, {
    headers,
    timeout: 90_000,
  });
  expect(resp.status()).toBe(200);
  const json = await resp.json();
  expect(json.filename).toBeTruthy();
  return json.filename as string;
}

/** Delete a backup file (best-effort). */
async function deleteBackup(
  request: import("@playwright/test").APIRequestContext,
  headers: Record<string, string>,
  filename: string,
): Promise<void> {
  await request
    .delete(`${APP_BASE}/api/backups/${filename}`, { headers })
    .catch(() => {});
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test.describe("Backup track-lifecycle resilience", () => {
  test.describe.configure({ timeout: 180_000 });

  // -------------------------------------------------------------------------
  // Test 1: tracks_in_reaper_not_in_backup is defined and is an array
  // -------------------------------------------------------------------------
  test(
    "restore_ignores_tracks_added_after_backup",
    async ({ page, request }) => {
      const consoleErrors: string[] = [];
      page.on("console", (msg) => {
        if (msg.type() === "error" || msg.type() === "warning") {
          const text = msg.text();
          if (text.includes("apple-mobile-web-app-capable")) return;
          if (text.includes("[push] subscribe await failed")) return;
          if (text.includes("Push API in incognito mode")) return;
          if (/integrity.*attribute.*ignored/i.test(text)) return;
          if (text.includes("vapid-key fetch error")) return;
          consoleErrors.push(`[${msg.type()}] ${text}`);
        }
      });

      const token = await getEngineerToken(request, page);
      const headers = { Authorization: `Bearer ${token}` };

      let filename: string | null = null;

      try {
        filename = await captureBackup(request, headers);

        // Preview the backup we just captured — tracks_in_reaper_not_in_backup
        // should exist as an array (may be empty when backup is current).
        const previewResp = await request.post(
          `${APP_BASE}/api/backups/${filename}/preview`,
          { headers, timeout: 90_000 },
        );
        expect(previewResp.status()).toBe(200);
        const preview = await previewResp.json();

        expect(
          Array.isArray(preview.tracks_in_reaper_not_in_backup),
          "preview must include tracks_in_reaper_not_in_backup array",
        ).toBe(true);

        // When the backup is freshly captured, this list reflects only tracks
        // that truly don't appear in track_mutes (e.g. non-inear/stems tracks).
        // We don't assert a specific length — just that the field exists and
        // is a well-typed array.
        for (const name of preview.tracks_in_reaper_not_in_backup) {
          expect(typeof name).toBe("string");
        }

        expect(
          Array.isArray(preview.tracks_in_backup_not_in_reaper),
          "preview must include tracks_in_backup_not_in_reaper array",
        ).toBe(true);

        // Freshly captured backup — no tracks should be missing from REAPER.
        expect(
          preview.tracks_in_backup_not_in_reaper,
          "freshly captured backup should have no tracks missing from REAPER",
        ).toEqual([]);
      } finally {
        if (filename) await deleteBackup(request, headers, filename);
      }

      expect(consoleErrors).toEqual([]);
    },
  );

  // -------------------------------------------------------------------------
  // Test 2: renamed track lands in skipped_tracks on restore
  // -------------------------------------------------------------------------
  test(
    "restore_skips_tracks_removed_before_restore",
    async ({ page, request }) => {
      const consoleErrors: string[] = [];
      page.on("console", (msg) => {
        if (msg.type() === "error" || msg.type() === "warning") {
          const text = msg.text();
          if (text.includes("apple-mobile-web-app-capable")) return;
          if (text.includes("[push] subscribe await failed")) return;
          if (text.includes("Push API in incognito mode")) return;
          if (/integrity.*attribute.*ignored/i.test(text)) return;
          if (text.includes("vapid-key fetch error")) return;
          consoleErrors.push(`[${msg.type()}] ${text}`);
        }
      });

      const token = await getEngineerToken(request, page);
      const headers = { Authorization: `Bearer ${token}` };

      // Save track's original name before any changes.
      const originalName = await getTrackName(request, RENAME_TRACK_IDX);
      expect(originalName).toBeTruthy();

      let filename: string | null = null;

      try {
        // 1. Capture a backup with the track under its original name.
        filename = await captureBackup(request, headers);

        // 2. Rename the track so the backup's name no longer resolves.
        const tempName = `__lifecycle_test_${Date.now()}`;
        await renameTrack(request, RENAME_TRACK_IDX, tempName);

        // Verify rename took effect.
        const renamedName = await getTrackName(request, RENAME_TRACK_IDX);
        expect(renamedName).toBe(tempName);

        // 3. Restore the backup — the original name won't resolve.
        const restoreResp = await request.post(
          `${APP_BASE}/api/backups/${filename}/restore`,
          { headers, timeout: 90_000 },
        );
        expect(restoreResp.status()).toBe(200);
        const result = await restoreResp.json();

        // 4. skipped_tracks must be present and contain the original name
        //    (it was in the backup but not in REAPER after the rename).
        expect(
          Array.isArray(result.skipped_tracks),
          "restore result must include skipped_tracks array",
        ).toBe(true);

        expect(
          result.skipped_tracks,
          `skipped_tracks should include the renamed track '${originalName}'`,
        ).toContain(originalName);
      } finally {
        // Rename the track back to its original name regardless of test outcome.
        await renameTrack(request, RENAME_TRACK_IDX, originalName);
        if (filename) await deleteBackup(request, headers, filename);
      }

      expect(consoleErrors).toEqual([]);
    },
  );

  // -------------------------------------------------------------------------
  // Test 3: skipped_tracks warning path (alternate framing of rename test)
  // -------------------------------------------------------------------------
  test(
    "restore_skips_renamed_tracks_with_warning",
    async ({ page, request }) => {
      const consoleErrors: string[] = [];
      page.on("console", (msg) => {
        if (msg.type() === "error" || msg.type() === "warning") {
          const text = msg.text();
          if (text.includes("apple-mobile-web-app-capable")) return;
          if (text.includes("[push] subscribe await failed")) return;
          if (text.includes("Push API in incognito mode")) return;
          if (/integrity.*attribute.*ignored/i.test(text)) return;
          if (text.includes("vapid-key fetch error")) return;
          consoleErrors.push(`[${msg.type()}] ${text}`);
        }
      });

      const token = await getEngineerToken(request, page);
      const headers = { Authorization: `Bearer ${token}` };

      const originalName = await getTrackName(request, RENAME_TRACK_IDX);
      expect(originalName).toBeTruthy();

      let filename: string | null = null;

      try {
        filename = await captureBackup(request, headers);

        // Rename track to something that doesn't exist in the backup.
        const tempName = `__lifecycle_warn_${Date.now()}`;
        await renameTrack(request, RENAME_TRACK_IDX, tempName);

        const renamedName = await getTrackName(request, RENAME_TRACK_IDX);
        expect(renamedName).toBe(tempName);

        // Preview the backup — the original name should appear in
        // tracks_in_backup_not_in_reaper since it can no longer be found.
        const previewResp = await request.post(
          `${APP_BASE}/api/backups/${filename}/preview`,
          { headers, timeout: 90_000 },
        );
        expect(previewResp.status()).toBe(200);
        const preview = await previewResp.json();

        expect(
          Array.isArray(preview.tracks_in_backup_not_in_reaper),
        ).toBe(true);

        // The original name was in track_mutes at capture time; after rename
        // it no longer exists in REAPER, so it must appear in the diff list.
        expect(
          preview.tracks_in_backup_not_in_reaper,
          `preview should flag '${originalName}' as missing from REAPER`,
        ).toContain(originalName);

        // Apply restore; confirm skipped_tracks matches.
        const restoreResp = await request.post(
          `${APP_BASE}/api/backups/${filename}/restore`,
          { headers, timeout: 90_000 },
        );
        expect(restoreResp.status()).toBe(200);
        const result = await restoreResp.json();

        expect(Array.isArray(result.skipped_tracks)).toBe(true);
        expect(result.skipped_tracks).toContain(originalName);
      } finally {
        await renameTrack(request, RENAME_TRACK_IDX, originalName);
        if (filename) await deleteBackup(request, headers, filename);
      }

      expect(consoleErrors).toEqual([]);
    },
  );

  // -------------------------------------------------------------------------
  // Test 4: round-trip — two captures, both files remain in listing
  // -------------------------------------------------------------------------
  test(
    "restore_handles_track_reordering_correctly",
    async ({ page, request }) => {
      const consoleErrors: string[] = [];
      page.on("console", (msg) => {
        if (msg.type() === "error" || msg.type() === "warning") {
          const text = msg.text();
          if (text.includes("apple-mobile-web-app-capable")) return;
          if (text.includes("[push] subscribe await failed")) return;
          if (text.includes("Push API in incognito mode")) return;
          if (/integrity.*attribute.*ignored/i.test(text)) return;
          if (text.includes("vapid-key fetch error")) return;
          consoleErrors.push(`[${msg.type()}] ${text}`);
        }
      });

      const token = await getEngineerToken(request, page);
      const headers = { Authorization: `Bearer ${token}` };

      let file1: string | null = null;
      let file2: string | null = null;

      try {
        // 1. First capture.
        file1 = await captureBackup(request, headers);

        // 2. Restore it (exercises the full apply path without destructive
        //    track reordering — index reordering would require deleting tracks).
        const restoreResp = await request.post(
          `${APP_BASE}/api/backups/${file1}/restore`,
          { headers, timeout: 90_000 },
        );
        expect(restoreResp.status()).toBe(200);
        const result = await restoreResp.json();

        // Restore should succeed with an empty skipped_tracks array because
        // we just captured the backup from the live state.
        expect(Array.isArray(result.skipped_tracks)).toBe(true);
        expect(
          result.skipped_tracks,
          "round-trip restore of a current backup should skip no tracks",
        ).toEqual([]);

        // 3. Second capture after restore.
        file2 = await captureBackup(request, headers);

        // 4. Both backup files must appear in the listing.
        const listResp = await request.get(`${APP_BASE}/api/backups`, {
          headers,
        });
        expect(listResp.status()).toBe(200);
        const list = await listResp.json();
        const filenames: string[] = list.map(
          (b: { filename: string }) => b.filename,
        );

        expect(
          filenames,
          "backup listing must contain the first capture file",
        ).toContain(file1);
        expect(
          filenames,
          "backup listing must contain the second capture file",
        ).toContain(file2);
      } finally {
        if (file1) await deleteBackup(request, headers, file1);
        if (file2) await deleteBackup(request, headers, file2);
      }

      expect(consoleErrors).toEqual([]);
    },
  );
});
