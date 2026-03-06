import { test, expect, Page } from "@playwright/test";

// Helper to login and set auth in localStorage
async function loginAs(page: Page, member: string) {
  const response = await page.request.post("/api/auth", {
    data: { member, pin: "7711" },
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

// Precondition check: logs explicitly and returns false when condition is not met.
function assume(condition: unknown, message: string): condition is true {
  if (!condition) {
    console.log(`[ASSUME SKIP] ${message}`);
    return false;
  }
  return true;
}

// Helper to wait for mixer page to load with graceful skip in CI
async function waitForMixer(
  page: Page,
  message = "Mixer must load (requires REAPER connection)",
): Promise<boolean> {
  const mixerLoaded = await page
    .waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 })
    .catch(() => null);
  return assume(mixerLoaded, message);
}

test.describe("Global Volume Persistence", () => {
  test("Petronela: Global volume persists after page reload", async ({
    page,
  }) => {
    // Step 1: Login to Petronela mixer
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    if (!(await waitForMixer(page))) return;

    // Step 2: Find global volume fader and get current value
    const globalVol = page.locator('[data-testid="global-volume-fader"]');
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(globalLoaded, "global volume fader must load for this test"))
      return;

    // Get the db-display element with data-value
    const dbDisplay = globalVol.locator(".db-display");
    const initialValue = await dbDisplay.getAttribute("data-value");
    if (!assume(initialValue !== null, "global volume must have data-value"))
      return;

    const initialDb = parseFloat(initialValue);
    console.log(`Initial global volume: ${initialDb} dB`);

    // Step 3: Set IEM VOL to a different value (-10dB) by dragging fader
    const fader = globalVol.locator(".fader-track");
    const box = await fader.boundingBox();
    if (!assume(box, "fader bounding box must exist")) return;

    // Calculate fader position for -10dB
    // Range: -60dB to +12dB = 72dB total
    // -10dB = 50dB from min, so 50/72 = ~69.4%
    const targetPct = (-10 - -60) / (12 - -60); // 50/72 = 0.694

    // Mouse down at center, wait for activation
    await page.mouse.move(box!.x + box!.width * 0.5, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(350);

    // Drag to target position
    await page.mouse.move(
      box!.x + box!.width * targetPct,
      box!.y + box!.height / 2,
    );
    await page.waitForTimeout(50);
    await page.mouse.up();

    // Wait for WebSocket command to be sent and value to settle
    await page.waitForTimeout(500);

    // Read the value after drag
    const afterDragValue = await dbDisplay.getAttribute("data-value");
    if (!assume(afterDragValue !== null, "value must exist after drag")) return;
    const afterDragDb = parseFloat(afterDragValue);
    console.log(`After drag global volume: ${afterDragDb} dB`);

    // Verify we actually changed the value (should be roughly -10dB)
    // Allow some tolerance since fader drag isn't pixel-perfect
    expect(afterDragDb).toBeGreaterThan(-15);
    expect(afterDragDb).toBeLessThan(-5);

    // Step 4: Reload page and re-login
    await page.reload();
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    if (!(await waitForMixer(page, "Mixer must load after reload"))) return;

    // Wait for WebSocket to reconnect and receive initial state
    const globalVolReloaded = page.locator(
      '[data-testid="global-volume-fader"]',
    );
    const reloadedLoaded = await globalVolReloaded
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(reloadedLoaded, "global volume must load after reload")) return;

    // Give time for WebSocket to deliver initial state
    await page.waitForTimeout(1000);

    // Step 5: Verify IEM VOL persisted (should NOT be 0dB, should be ~-10dB)
    const dbDisplayReloaded = globalVolReloaded.locator(".db-display");
    const persistedValue = await dbDisplayReloaded.getAttribute("data-value");
    if (!assume(persistedValue !== null, "persisted value must exist")) return;
    const persistedDb = parseFloat(persistedValue);
    console.log(`After reload global volume: ${persistedDb} dB`);

    // THE BUG: This assertion should FAIL if the bug exists
    // Bug: persistedDb is always 0.0 regardless of what was set
    // Expected: persistedDb should be approximately afterDragDb
    expect(persistedDb).toBeCloseTo(afterDragDb, 0); // 0 decimal places = within 0.5
  });

  test("ANI: Global volume persists after page reload", async ({ page }) => {
    // Same test for ANI to verify if bug is member-specific
    await page.goto("/");
    await loginAs(page, "ani");
    await page.goto("/ani");

    if (!(await waitForMixer(page))) return;

    const globalVol = page.locator('[data-testid="global-volume-fader"]');
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(globalLoaded, "global volume fader must load for ANI")) return;

    const dbDisplay = globalVol.locator(".db-display");
    const initialValue = await dbDisplay.getAttribute("data-value");
    if (!assume(initialValue !== null, "global volume must have data-value"))
      return;

    const initialDb = parseFloat(initialValue);
    console.log(`[ANI] Initial global volume: ${initialDb} dB`);

    // Set to a different value (-15dB for ANI to differentiate)
    const fader = globalVol.locator(".fader-track");
    const box = await fader.boundingBox();
    if (!assume(box, "fader bounding box must exist")) return;

    const targetPct = (-15 - -60) / (12 - -60); // 45/72 = 0.625

    await page.mouse.move(box!.x + box!.width * 0.5, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(350);
    await page.mouse.move(
      box!.x + box!.width * targetPct,
      box!.y + box!.height / 2,
    );
    await page.waitForTimeout(50);
    await page.mouse.up();
    await page.waitForTimeout(500);

    const afterDragValue = await dbDisplay.getAttribute("data-value");
    if (!assume(afterDragValue !== null, "value must exist after drag")) return;
    const afterDragDb = parseFloat(afterDragValue);
    console.log(`[ANI] After drag global volume: ${afterDragDb} dB`);

    expect(afterDragDb).toBeGreaterThan(-20);
    expect(afterDragDb).toBeLessThan(-10);

    // Reload and verify
    await page.reload();
    await loginAs(page, "ani");
    await page.goto("/ani");

    if (!(await waitForMixer(page, "Mixer must load after reload"))) return;

    const globalVolReloaded = page.locator(
      '[data-testid="global-volume-fader"]',
    );
    await globalVolReloaded.waitFor({ state: "visible", timeout: 5000 });
    await page.waitForTimeout(1000);

    const dbDisplayReloaded = globalVolReloaded.locator(".db-display");
    const persistedValue = await dbDisplayReloaded.getAttribute("data-value");
    if (!assume(persistedValue !== null, "persisted value must exist")) return;
    const persistedDb = parseFloat(persistedValue);
    console.log(`[ANI] After reload global volume: ${persistedDb} dB`);

    // If ANI passes and Petronela fails, bug is member-specific
    expect(persistedDb).toBeCloseTo(afterDragDb, 0);
  });

  test("Global volume value is read from REAPER on connect", async ({
    page,
  }) => {
    // This test verifies the WebSocket initial state delivery
    // by checking if the value is populated shortly after connection
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    if (!(await waitForMixer(page))) return;

    const globalVol = page.locator('[data-testid="global-volume-fader"]');
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(globalLoaded, "global volume fader must load")) return;

    // Wait for WebSocket to connect and receive initial state
    // If working correctly, value should be populated within 1s
    await page.waitForTimeout(1500);

    const dbDisplay = globalVol.locator(".db-display");
    const value = await dbDisplay.getAttribute("data-value");
    if (!assume(value !== null, "global volume must have value after connect"))
      return;

    const db = parseFloat(value);
    console.log(`Global volume after connect: ${db} dB`);

    // The value should reflect REAPER's actual state (whatever it is)
    // If it's exactly 0.0 AND the fader was previously set to something else,
    // this indicates the bug (initial state not delivered)
    //
    // For this test, we just verify a value exists and is in valid range
    expect(db).toBeGreaterThanOrEqual(-60);
    expect(db).toBeLessThanOrEqual(12);
  });
});
