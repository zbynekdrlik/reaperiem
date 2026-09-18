/**
 * #205 — Preset save must capture EQ server-side for ALL tracks.
 *
 * Before the fix, a preset stored only the EQ the client happened to send
 * (one track, only if its EQ modal was open this session) — so EQ was
 * silently lost on almost every save. The fix captures EQ live on the server
 * (same helper the snapshot path uses) for every channel in the request.
 *
 * This test posts a preset with NO eq_bands in the request body (mirroring the
 * old client that sent none) and asserts the STORED preset comes back with
 * eq_bands populated — i.e. the server captured them. RED against the old
 * server (eq_bands absent), GREEN after the fix.
 *
 * Safety: engineer login (PIN 1177). The test only creates and deletes an
 * engineer preset FILE — it never restores it, so no REAPER send/EQ is
 * modified. The probe preset is deleted in `finally`.
 */

import { test, expect } from "@playwright/test";
import { ENGINEER_PIN } from "../fixtures/test-credentials";

const PROBE_NAME = "e2e_eq_capture_probe";

test.describe.configure({ timeout: 120_000 });

test.describe("#205 preset save captures EQ server-side", () => {
  test("saving a preset with no eq_bands still stores captured EQ", async ({
    page,
  }) => {
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

    await page.goto("/");
    const authResp = await page.request.post("/api/auth", {
      data: { member: "engineer", pin: ENGINEER_PIN },
    });
    expect(authResp.status()).toBe(200);
    const { token } = await authResp.json();
    const headers = { Authorization: `Bearer ${token}` };

    // Fetch the engineer mixer so we know real track indices to save.
    let channels: Array<{ track_index: number; level_db: number; pan: number; muted: boolean }> =
      [];
    for (let attempt = 0; attempt < 10; attempt++) {
      const mixerResp = await page.request.get("/api/mixer/engineer", { headers });
      expect(mixerResp.ok()).toBeTruthy();
      const data = await mixerResp.json();
      channels = data.channels ?? [];
      if (channels.length > 0) break;
      await page.waitForTimeout(1000);
    }
    expect(channels.length, "engineer mixer must expose channels").toBeGreaterThan(0);

    // Build the request WITHOUT eq_bands (the old client sent none / partial).
    const channelMap: Record<string, { vol: number; mute: boolean; pan: number }> = {};
    for (const ch of channels) {
      channelMap[String(ch.track_index)] = {
        vol: ch.level_db,
        mute: ch.muted,
        pan: ch.pan,
      };
    }

    try {
      const saveResp = await page.request.post("/api/presets/engineer", {
        headers,
        data: { name: PROBE_NAME, channels: channelMap },
      });
      expect(
        saveResp.status(),
        `preset save failed: ${await saveResp.text()}`,
      ).toBe(201);

      const getResp = await page.request.get(
        `/api/presets/engineer/${encodeURIComponent(PROBE_NAME)}`,
        { headers },
      );
      expect(getResp.ok()).toBeTruthy();
      const preset = await getResp.json();

      // #205: even though the request carried NO eq_bands, the server must have
      // captured EQ live for the tracks that have a ReaEQ. Before the fix this
      // was null/absent (silent EQ loss).
      expect(
        preset.eq_bands,
        "#205: server must capture EQ on save even when the client sends none",
      ).toBeTruthy();
      expect(
        Object.keys(preset.eq_bands ?? {}).length,
        "#205: at least one track's EQ must be captured server-side",
      ).toBeGreaterThan(0);
    } finally {
      await page.request
        .delete(`/api/presets/engineer/${encodeURIComponent(PROBE_NAME)}`, {
          headers,
        })
        .catch(() => {});
    }

    expect(consoleErrors).toEqual([]);
  });
});
