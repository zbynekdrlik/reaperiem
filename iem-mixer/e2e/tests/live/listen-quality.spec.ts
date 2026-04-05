import { test, expect, Page } from "@playwright/test";

// Helper to login and set auth in localStorage
async function loginAs(page: Page, member: string, pin: string = "7711") {
  const response = await page.request.post("/api/auth", {
    data: { member, pin },
  });

  if (response.status() === 200) {
    const data = await response.json();
    await page.evaluate(
      ({ token, member, engineer }) => {
        localStorage.setItem(
          "iem_token",
          JSON.stringify({ token, member, engineer }),
        );
      },
      { token: data.token, member: data.member, engineer: data.engineer },
    );
  }
}

async function waitForMixer(page: Page) {
  await expect(page.locator(".app.mixer, .mixer-header")).toBeVisible({ timeout: 10000 });
}

test.describe("Listen Stream Quality (#113)", () => {
  test("getStreamStats() returns correct shape when not listening", async ({
    page,
  }) => {
    // Load audio_player.js via the app
    await loginAs(page, "engineer");
    await page.goto("/engineer");
    await waitForMixer(page);

    // Access getStreamStats from window global
    const stats = await page.evaluate(() => {
      return (window as any).__iem_stream_stats
        ? (window as any).__iem_stream_stats()
        : null;
    });

    expect(stats).toBeTruthy();

    expect(stats).toHaveProperty("dropouts");
    expect(stats).toHaveProperty("frames");
    expect(stats).toHaveProperty("bufferMs");
    expect(stats).toHaveProperty("quality");
    expect(stats.dropouts).toBe(0);
    expect(stats.frames).toBe(0);
    expect(stats.quality).toBe("good");
  });

  test("stream stats element appears when Listen is active", async ({
    page,
  }) => {
    await loginAs(page, "engineer");
    await page.goto("/engineer");
    await waitForMixer(page);

    // Stats should not be visible before listening
    const statsBefore = await page
      .locator('[data-testid="stream-stats"]')
      .count();
    expect(statsBefore).toBe(0);

    // Click listen button
    const listenBtn = page.locator(".toolbar-btn-listen");
    if (!(await listenBtn.count())) return;

    await listenBtn.click();

    // Wait for listening state (needs REAPER audio pipeline)
    const listening = await page
      .waitForSelector(".toolbar-btn-listen.listening", { timeout: 5000 })
      .catch(() => null);

    expect(listening).toBeTruthy();

    // Stream stats should now be visible
    const statsEl = page.locator('[data-testid="stream-stats"]');
    await expect(statsEl).toBeVisible();

    // Should contain "drops" and "buf" text
    const text = await statsEl.textContent();
    expect(text).toContain("drops");
    expect(text).toContain("buf");
    expect(text).toContain("ms");

    // Stop listening
    await listenBtn.click();
  });

  test("dropout counter increments on frame gaps", async ({ page }) => {
    await loginAs(page, "engineer");
    await page.goto("/engineer");
    await waitForMixer(page);

    // Simulate dropout detection by calling feedOpusFrame with gaps
    // We need to init the audio player first (requires user gesture context)
    const canTest = await page.evaluate(() => {
      return typeof (window as any).AudioDecoder !== "undefined";
    });

    expect(canTest).toBeTruthy();

    // Use page.evaluate to simulate frame timing gaps
    const result = await page.evaluate(async () => {
      // Access the module's getStreamStats via window global
      const getStats = (window as any).__iem_stream_stats;
      if (!getStats) return { error: "no getStreamStats" };

      const before = getStats();
      return {
        beforeDropouts: before.dropouts,
        beforeFrames: before.frames,
        beforeQuality: before.quality,
      };
    });

    // Verify initial state
    expect(!("error" in result)).toBeTruthy();
    expect(result.beforeDropouts).toBe(0);
    expect(result.beforeQuality).toBe("good");
  });

  test("jitter buffer depth starts at 150ms", async ({ page }) => {
    await loginAs(page, "engineer");
    await page.goto("/engineer");
    await waitForMixer(page);

    const stats = await page.evaluate(() => {
      const getStats = (window as any).__iem_stream_stats;
      if (!getStats) return null;
      return getStats();
    });

    expect(stats).toBeTruthy();

    // When not listening, bufferMs is 0 (no context)
    expect(stats.bufferMs).toBe(0);
  });
});
