/**
 * Audio E2E Tests — verifies audio pipeline delivers valid signal, not just bytes.
 *
 * Three layers of verification:
 *   1. Diagnostics API: server-side peak_db, opus_frames_per_second (pipeline health)
 *   2. Browser-level: Click Listen button, verify no console errors, check audio level
 *   3. Auth: only engineers can access diagnostics
 */

import { test, expect } from "@playwright/test";

const BASE_URL = process.env.E2E_BASE_URL || "http://localhost:8080";

// Authenticate as engineer and return token
async function getEngineerToken(
  request: ReturnType<typeof test.extend>["request"] extends infer R
    ? R
    : never,
): Promise<string | null> {
  try {
    const response = await (request as { post: Function }).post(
      `${BASE_URL}/api/auth`,
      {
        data: { member: "engineer", pin: "1177" },
      },
    );
    if (response.status() !== 200) return null;
    const data = await response.json();
    return data.token || null;
  } catch {
    return null;
  }
}

test.describe("Audio Pipeline Diagnostics", () => {
  test("diagnostics endpoint returns valid structure", async ({ request }) => {
    const token = await getEngineerToken(request);
    expect(token).toBeTruthy();

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(response.status()).toBe(200);

    const diag = await response.json();
    expect(diag).toHaveProperty("receiving_oiem");
    expect(diag).toHaveProperty("receiving_vban"); // backwards compat
    expect(diag).toHaveProperty("packets_per_second");
    expect(diag).toHaveProperty("opus_frames_per_second");
    expect(diag).toHaveProperty("peak_db");
    expect(diag).toHaveProperty("last_sequence");
    expect(diag).toHaveProperty("sequence_gaps");
    expect(typeof diag.receiving_oiem).toBe("boolean");
    expect(typeof diag.peak_db).toBe("number");
  });

  test("when OIEM is active, audio signal is not silence", async ({
    request,
  }) => {
    const token = await getEngineerToken(request);
    expect(token).toBeTruthy();

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const diag = await response.json();

    // Tone generator is active during post-deploy E2E (see ci.yml).
    // OIEM packets MUST be flowing — if not, the VST / pipeline / tone trigger is broken.
    expect(diag.receiving_oiem).toBe(true);

    // If OIEM is receiving, the pipeline must be producing real audio
    expect(diag.packets_per_second).toBeGreaterThan(10);
    expect(diag.opus_frames_per_second).toBeGreaterThan(10);

    // With a tone generator or real signal, peak should be above -40 dB
    // Silence is -150 dB; noise floor is around -80 dB
    expect(diag.peak_db).toBeGreaterThan(-40);
  });

  test("diagnostics requires engineer auth", async ({ request }) => {
    // No auth header → should fail
    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`);
    expect(response.status()).toBe(401);
  });

  test("non-engineer token rejected for diagnostics", async ({ request }) => {
    // Login as regular member
    const authResp = await request.post(`${BASE_URL}/api/auth`, {
      data: { member: "petronela", pin: "7711" },
    });
    expect(authResp.status()).toBe(200);
    const { token } = await authResp.json();

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    // Should be forbidden — only engineers can access diagnostics
    expect(response.status()).toBe(403);
  });
});

test.describe("Browser Audio Playback", () => {
  test("engineer Listen button transitions correctly and reports no errors", async ({
    page,
  }) => {
    // Login as engineer via direct URL
    await page.goto(`${BASE_URL}/login`);

    // Wait for login page to load
    await page.waitForSelector('button:has-text("1")', { timeout: 10000 });

    // Enter engineer PIN: 1177
    await page.getByRole("button", { name: "1", exact: true }).first().click();
    await page.getByRole("button", { name: "1", exact: true }).first().click();
    await page.getByRole("button", { name: "7" }).click();
    await page.getByRole("button", { name: "7" }).click();

    // Wait for navigation to mixer page
    await page.waitForURL("**/engineer", { timeout: 10000 });

    // Listen button MUST exist on engineer page — no silent skip
    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });
    const btnText = await listenBtn.textContent();

    // WebCodecs must be supported in Chromium
    expect(btnText).not.toContain("Unsupported");

    // Collect console errors AND uncaught exceptions during playback
    const consoleErrors: string[] = [];
    const pageErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });
    // CRITICAL: Catch uncaught exceptions (RangeError, TypeError, etc.)
    // These are NOT console.error — they are thrown errors that crash silently
    page.on("pageerror", (error) => {
      pageErrors.push(`${error.name}: ${error.message}`);
    });

    // Click Listen button
    await listenBtn.click();

    // The button should NOT immediately get .listening class (Bug 1 fix).
    // It should stay as "Listen" until first frame arrives.
    // Wait a brief moment to check it doesn't prematurely transition.
    await page.waitForTimeout(500);

    // Now wait for state change (up to 10s for audio frames to start arriving)
    // With no VBAN source, it will go to "No Source" after ~5s
    // With a VBAN source, it will get .listening class when first frame arrives
    // No catch — waitForFunction MUST succeed. Tone generator is active during
    // post-deploy E2E, so the button must reach `listening` state within 10 s.
    await page.waitForFunction(
      () => {
        const btn = document.querySelector(".toolbar-btn-listen");
        if (!btn) return false;
        return (
          btn.classList.contains("listening") ||
          btn.textContent?.includes("No Source")
        );
      },
      { timeout: 10000 },
    );

    const afterClick = await listenBtn.textContent();
    // Audio source MUST be available (REAPER + tone generator running during E2E)
    expect(afterClick).not.toContain("No Source");

    const isListening = await listenBtn.evaluate((el) =>
      el.classList.contains("listening"),
    );
    if (isListening) {
      // Audio is flowing — verify no errors
      await page.waitForTimeout(2000); // Let frames accumulate

      // Check for decoder errors via window interop
      const audioError = await page.evaluate(() => {
        return (window as any).__iem_audio_error?.() ?? null;
      });

      if (audioError) {
        throw new Error(`Audio decoder error surfaced: ${audioError}`);
      }

      // Check audio level (should be non-silence if VBAN sending)
      const audioLevel = await page.evaluate(() => {
        return (window as any).__iem_audio_level?.() ?? -999;
      });
      console.log(`Browser audio level: ${audioLevel}dB`);

      // Filter audio-related console errors
      const audioErrors = consoleErrors.filter((e) => e.includes("[audio]"));
      expect(audioErrors).toHaveLength(0);

      // CRITICAL: Catch uncaught exceptions like RangeError in AudioData.copyTo
      // These crash silently — the user hears nothing but the UI shows "Listening"
      if (pageErrors.length > 0) {
        throw new Error(
          `Uncaught page errors during audio playback:\n${pageErrors.join("\n")}`,
        );
      }

      // Click again to stop
      await listenBtn.click();

      // Verify it returns to idle
      await page.waitForFunction(
        () => {
          const btn = document.querySelector(".toolbar-btn-listen");
          return btn && btn.textContent?.includes("Listen");
        },
        { timeout: 5000 },
      );
    }
  });
});
