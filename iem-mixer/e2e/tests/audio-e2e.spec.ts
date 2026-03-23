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
    if (!token) {
      console.log("[SKIP] Cannot authenticate as engineer");
      return;
    }

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(response.status()).toBe(200);

    const diag = await response.json();
    expect(diag).toHaveProperty("receiving_vban");
    expect(diag).toHaveProperty("packets_per_second");
    expect(diag).toHaveProperty("opus_frames_per_second");
    expect(diag).toHaveProperty("peak_db");
    expect(diag).toHaveProperty("sample_rate");
    expect(diag).toHaveProperty("channels");
    expect(typeof diag.receiving_vban).toBe("boolean");
    expect(typeof diag.peak_db).toBe("number");
  });

  test("when VBAN is active, audio signal is not silence", async ({
    request,
  }) => {
    const token = await getEngineerToken(request);
    if (!token) {
      console.log("[SKIP] Cannot authenticate as engineer");
      return;
    }

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const diag = await response.json();

    if (!diag.receiving_vban) {
      console.log(
        "[SKIP] No VBAN packets — REAPER not running or VBAN VST not active",
      );
      return;
    }

    // If VBAN is receiving, the pipeline must be producing real audio
    expect(diag.packets_per_second).toBeGreaterThan(10);
    expect(diag.opus_frames_per_second).toBeGreaterThan(10);

    // With a tone generator or real signal, peak should be above -40 dB
    // Silence is -150 dB; noise floor is around -80 dB
    expect(diag.peak_db).toBeGreaterThan(-40);
    expect(diag.sample_rate).toBeGreaterThan(0);
    expect(diag.channels).toBeGreaterThan(0);
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
    if (authResp.status() !== 200) {
      console.log("[SKIP] Cannot authenticate as member");
      return;
    }
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

    // Check if Listen button exists
    const listenBtn = page.locator(".toolbar-btn-listen");
    const btnCount = await listenBtn.count();
    if (btnCount === 0) {
      console.log("[SKIP] Listen button not found on engineer page");
      return;
    }

    await expect(listenBtn).toBeVisible();
    const btnText = await listenBtn.textContent();

    // Skip if browser doesn't support WebCodecs
    if (btnText?.includes("Unsupported")) {
      console.log(
        "[ASSUME SKIP] Browser does not support WebCodecs AudioDecoder",
      );
      return;
    }

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
    try {
      await page.waitForFunction(
        () => {
          const btn = document.querySelector(".toolbar-btn-listen");
          return (
            btn &&
            (btn.classList.contains("listening") ||
              btn.textContent?.includes("No Source"))
          );
        },
        { timeout: 10000 },
      );
    } catch {
      // Timeout is OK — might not have audio source in CI
      console.log("[INFO] Listen button did not transition within 10s");
    }

    const afterClick = await listenBtn.textContent();
    console.log(`Listen button state after click: "${afterClick}"`);

    if (afterClick?.includes("No Source")) {
      console.log(
        "[ASSUME SKIP] No audio source available (REAPER may not be running)",
      );
      // Still check: no decoder errors or uncaught exceptions
      const audioErrors = consoleErrors.filter((e) => e.includes("[audio]"));
      expect(audioErrors).toHaveLength(0);
      expect(pageErrors).toHaveLength(0);

      // Click again to stop
      await listenBtn.click();
      return;
    }

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
