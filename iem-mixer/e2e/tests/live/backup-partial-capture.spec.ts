/**
 * RED reproducer for "capture refuses partial backups" (defensive hardening).
 *
 * Even though the 21.4 backup incident was not caused by a partial capture,
 * the system currently has NO assertion that captures are complete. A future
 * REAPER hiccup during a daemon run could silently produce a corrupt backup
 * with too few sends or track-mute entries, and the system would accept it.
 *
 * This test MUST FAIL against current code (RED) because:
 *   - /api/backups/capture does not yet return an `audit` object
 *   - CaptureAudit and assert_capture_completeness do not exist yet
 *
 * Task 6 (GREEN) will add CaptureAudit to iem-core, assert_capture_completeness
 * to backup_capture.rs, and return audit counts in the capture API response.
 *
 * Safety: engineer-only writes; finally-block deletes the created backup.
 */

import { test, expect } from "@playwright/test";

const APP = process.env.IEM_APP_URL || "http://10.77.9.231";

const MIN_SENDS = 200;
const MIN_TRACK_MUTES = 30;

test.describe("Capture coverage assertion (defensive hardening)", () => {
  // Backup capture reads EQ/limiter for all tracks via EXTSTATE — slow.
  test.describe.configure({ timeout: 120_000 });

  test(
    "capture_response_includes_audit_counts",
    async ({ page, request }) => {
      const consoleErrors: string[] = [];
      page.on("console", (msg) => {
        if (msg.type() === "error" || msg.type() === "warning") {
          const text = msg.text();
          // Known-benign browser notices — mirrors backup-cg-remute.spec.ts filter.
          if (text.includes("apple-mobile-web-app-capable")) return;
          if (text.includes("[push] subscribe await failed")) return;
          if (text.includes("Push API in incognito mode")) return;
          if (/integrity.*attribute.*ignored/i.test(text)) return;
          if (text.includes("vapid-key fetch error")) return;
          consoleErrors.push(`[${msg.type()}] ${text}`);
        }
      });

      // Engineer login — mirrors backup-cg-remute.spec.ts pattern exactly.
      await page.goto("/");
      const authResp = await page.request.post("/api/auth", {
        data: { member: "engineer", pin: "1177" },
      });
      expect(authResp.status()).toBe(200);
      const authData = await authResp.json();
      expect(authData.engineer).toBe(true);
      const token: string = authData.token;
      const headers = { Authorization: `Bearer ${token}` };

      let createdFilename: string | null = null;

      try {
        const cap = await page.request.post("/api/backups/capture", {
          headers,
          timeout: 90_000,
        });
        expect(cap.ok()).toBeTruthy();
        const capJson = await cap.json();
        createdFilename = capJson.filename as string;
        expect(createdFilename).toBeTruthy();
        expect(createdFilename).toMatch(/^\d{8}_\d{6}\.json$/);

        // The audit object must be present after T6's GREEN fix.
        // On current code this assertion fails with "audit is undefined".
        expect(
          capJson.audit,
          "capture response must include audit counts (RED: not implemented yet)",
        ).toBeDefined();
        expect(
          capJson.audit.sends_count,
          `sends_count must meet minimum ${MIN_SENDS}`,
        ).toBeGreaterThanOrEqual(MIN_SENDS);
        expect(
          capJson.audit.track_mutes_count,
          `track_mutes_count must meet minimum ${MIN_TRACK_MUTES}`,
        ).toBeGreaterThanOrEqual(MIN_TRACK_MUTES);
      } finally {
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
