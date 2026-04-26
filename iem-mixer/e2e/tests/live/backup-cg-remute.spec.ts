/**
 * RED reproducer for bug #4: global restore must re-mute the CG TRACK
 * when that track was muted at backup-capture time.
 *
 * Root cause (backup_capture.rs:171): the `track_mutes` filter only saves
 * tracks whose name contains "inear" or "stems". CG matches neither, so
 * its TRACK-level mute is never captured and therefore never restored.
 *
 * This test MUST FAIL against current code (RED). Task 4 will fix it GREEN.
 *
 * Safety: engineer-only writes, finally-block restores all state changes.
 */

import { test, expect } from "@playwright/test";

const REAPER = "http://iem.lan:8080";
const CG_TRACK_IDX = 45;

/** Read the TRACK-level mute bit for a given 1-based track index.
 *  `/_/GET/TRACK/{idx}` returns tab-separated:
 *    TRACK\tidx\tname\tflags\tvol\tpan\t...
 *  Track mute is bit 3 of the flags field (flags & 8 != 0).
 */
async function getTrackMute(
  request: import("@playwright/test").APIRequestContext,
  idx: number,
): Promise<boolean> {
  const r = await request.get(`${REAPER}/_/GET/TRACK/${idx}`);
  const text = await r.text();
  const parts = text.trim().split("\t");
  // Field layout: TRACK(0) idx(1) name(2) flags(3) vol(4) pan(5) ...
  const flags = parseInt(parts[3] ?? "0", 10);
  return (flags & 8) !== 0;
}

/** Set the TRACK-level mute for a given 1-based track index. */
async function setTrackMute(
  request: import("@playwright/test").APIRequestContext,
  idx: number,
  muted: boolean,
): Promise<void> {
  await request.get(`${REAPER}/_/SET/TRACK/${idx}/MUTE/${muted ? 1 : 0}`);
}

/** Read the raw mute flag byte for a send (field index 3 of the SEND line).
 *  Returns 0 (unmuted) or 8 (muted) per REAPER convention.
 */
async function getSendMuteRaw(
  request: import("@playwright/test").APIRequestContext,
  trackIdx: number,
  sendIdx: number,
): Promise<number> {
  const r = await request.get(
    `${REAPER}/_/GET/TRACK/${trackIdx}/SEND/${sendIdx}`,
  );
  const text = await r.text();
  const parts = text.trim().split("\t");
  // SEND(0) track(1) send(2) mute_flag(3) volume(4) pan(5) destination(6)
  return parseInt(parts[3] ?? "0", 10);
}

test.describe("Backup restore re-mutes CG TRACK after unmute", () => {
  // Backup capture reads EQ/limiter for all tracks via EXTSTATE — slow.
  test.describe.configure({ timeout: 120_000 });

  test(
    "global_restore_remutes_cg_track_when_track_was_muted_in_backup",
    async ({ page, request }) => {
      const consoleErrors: string[] = [];
      page.on("console", (msg) => {
        if (msg.type() === "error" || msg.type() === "warning") {
          const text = msg.text();
          // Known-benign browser notices — mirrors cg.spec.ts filter.
          if (text.includes("apple-mobile-web-app-capable")) return;
          if (text.includes("[push] subscribe await failed")) return;
          if (text.includes("Push API in incognito mode")) return;
          if (/integrity.*attribute.*ignored/i.test(text)) return;
          if (text.includes("vapid-key fetch error")) return;
          consoleErrors.push(`[${msg.type()}] ${text}`);
        }
      });

      // Get engineer JWT — mirrors backup.spec.ts getEngineerToken pattern.
      await page.goto("/");
      const authResp = await page.request.post("/api/auth", {
        data: { member: "engineer", pin: "1177" },
      });
      expect(authResp.status()).toBe(200);
      const authData = await authResp.json();
      expect(authData.engineer).toBe(true);
      const token: string = authData.token;
      const headers = { Authorization: `Bearer ${token}` };

      // 1. Capture baseline: CG TRACK mute + all 10 send mute states.
      const cgTrackMutedAtStart = await getTrackMute(request, CG_TRACK_IDX);
      const sendMutesAtStart: number[] = [];
      for (let i = 0; i < 10; i++) {
        sendMutesAtStart.push(await getSendMuteRaw(request, CG_TRACK_IDX, i));
      }

      let createdFilename: string | null = null;

      try {
        // 2. Force CG track to be MUTED so the backup captures that state.
        await setTrackMute(request, CG_TRACK_IDX, true);
        expect(
          await getTrackMute(request, CG_TRACK_IDX),
          "CG track should be muted after explicit REAPER SET MUTE=1",
        ).toBe(true);

        // 3. Engineer captures a backup with CG track muted.
        const capResp = await page.request.post("/api/backups/capture", {
          headers,
          timeout: 90_000,
        });
        expect(capResp.status()).toBe(200);
        const capJson = await capResp.json();
        createdFilename = capJson.filename as string;
        expect(createdFilename).toBeTruthy();
        expect(createdFilename).toMatch(/^\d{8}_\d{6}\.json$/);

        // 4. Force CG track to be UNMUTED — simulates the state to restore from.
        await setTrackMute(request, CG_TRACK_IDX, false);
        expect(
          await getTrackMute(request, CG_TRACK_IDX),
          "CG track should be unmuted after explicit REAPER SET MUTE=0",
        ).toBe(false);

        // 5. Restore the backup via API.
        const restoreResp = await page.request.post(
          `/api/backups/${createdFilename}/restore`,
          { headers, timeout: 90_000 },
        );
        expect(restoreResp.status()).toBe(200);

        // 6. Wait for restore to propagate to REAPER.
        await page.waitForTimeout(3000);

        // 7. Assert CG TRACK is muted again (matches what was captured).
        //    This assertion FAILS on current code because backup_capture.rs
        //    filters track_mutes to "inear"/"stems" only — CG is excluded.
        const cgTrackMutedAfter = await getTrackMute(request, CG_TRACK_IDX);
        expect(
          cgTrackMutedAfter,
          "BUG #4: CG track should be re-muted by restore (was muted at capture time). " +
            "Fails because backup_capture.rs `track_mutes` filter excludes non-inear/stems tracks.",
        ).toBe(true);
      } finally {
        // Always restore the original CG track mute state.
        await setTrackMute(request, CG_TRACK_IDX, cgTrackMutedAtStart);

        // Always restore original send mute states (defensive — restore may
        // have modified them, or the test may have aborted mid-way).
        for (let i = 0; i < 10; i++) {
          const wasMuted = sendMutesAtStart[i] !== 0;
          await request.get(
            `${REAPER}/_/SET/TRACK/${CG_TRACK_IDX}/SEND/${i}/MUTE/${wasMuted ? 1 : 0}`,
          );
        }

        // Delete the test backup (best-effort).
        if (createdFilename) {
          await page.request
            .delete(`/api/backups/${createdFilename}`, { headers })
            .catch(() => {});
        }
      }

      expect(consoleErrors).toEqual([]);
    },
  );
});
