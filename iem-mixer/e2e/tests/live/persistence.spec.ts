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

async function waitForMixer(page: Page) {
  await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible({ timeout: 10000 });
}

test.describe("Global Volume Persistence", () => {
  test("Petronela: Global volume persists after page reload", async ({
    page,
  }) => {
    // Step 1: Login to Petronela mixer
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    await waitForMixer(page);

    // Step 2: Find global volume fader and get current value
    const globalVol = page.locator('[data-testid="global-volume-fader"]');
    await expect(globalVol).toBeVisible({ timeout: 5000 });

    // Get the db-display element with data-value
    const dbDisplay = globalVol.locator(".db-display");
    const initialValue = await dbDisplay.getAttribute("data-value");
    expect(initialValue).not.toBeNull();

    const initialDb = parseFloat(initialValue);
    console.log(`Initial global volume: ${initialDb} dB`);

    // Step 3: Set IEM VOL to a different value (-10dB) by dragging fader
    const fader = globalVol.locator(".fader-track");
    const box = await fader.boundingBox();
    expect(box).toBeTruthy();

    // First drag to a known middle position to ensure room to move,
    // then drag to a target. Use 50% as start → 25% as target = decrease.
    // If fader is at an extreme, the first drag normalises it.

    // Step A: Drag to ~50% to normalise from any extreme position
    const startX = box!.x + box!.width * 0.5;
    const startY = box!.y + box!.height / 2;
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.waitForTimeout(350);
    await page.mouse.up();
    await page.waitForTimeout(500);

    // Step B: Now drag from center to left in increments (decrease volume)
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.waitForTimeout(350);
    for (let i = 1; i <= 10; i++) {
      await page.mouse.move(
        box!.x + box!.width * (0.5 - 0.025 * i),
        startY,
      );
      await page.waitForTimeout(50);
    }
    await page.mouse.up();

    // Wait for WebSocket command to be sent and value to settle
    await page.waitForTimeout(500);

    // Read the value after drag
    const afterDragValue = await dbDisplay.getAttribute("data-value");
    expect(afterDragValue).not.toBeNull();
    const afterDragDb = parseFloat(afterDragValue);
    console.log(`After drag global volume: ${afterDragDb} dB`);

    // Verify we actually changed the value from initial
    expect(afterDragDb).not.toBeCloseTo(initialDb, 0);

    // Step 4: Reload page and re-login
    await page.reload();
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    await waitForMixer(page);

    // Wait for WebSocket to reconnect and receive initial state
    const globalVolReloaded = page.locator(
      '[data-testid="global-volume-fader"]',
    );
    await expect(globalVolReloaded).toBeVisible({ timeout: 5000 });

    // Give time for WebSocket to deliver initial state
    await page.waitForTimeout(1000);

    // Step 5: Verify IEM VOL persisted (should NOT be 0dB, should be ~-10dB)
    const dbDisplayReloaded = globalVolReloaded.locator(".db-display");
    const persistedValue = await dbDisplayReloaded.getAttribute("data-value");
    expect(persistedValue).not.toBeNull();
    const persistedDb = parseFloat(persistedValue);
    console.log(`After reload global volume: ${persistedDb} dB`);

    // THE BUG: This assertion should FAIL if the bug exists
    // Bug: persistedDb is always 0.0 regardless of what was set
    // Expected: persistedDb should be approximately afterDragDb
    expect(persistedDb).toBeCloseTo(afterDragDb, 0); // 0 decimal places = within 0.5
  });

  test("Stevo: Global volume persists after page reload", async ({ page }) => {
    // Uses stevo (different member than test 1) to verify cross-member persistence
    await page.goto("/");
    await loginAs(page, "stevo");
    await page.goto("/stevo");

    await waitForMixer(page);

    const globalVol = page.locator('[data-testid="global-volume-fader"]');
    await expect(globalVol).toBeVisible({ timeout: 15000 });

    const dbDisplay = globalVol.locator(".db-display");
    const initialValue = await dbDisplay.getAttribute("data-value");
    expect(initialValue).not.toBeNull();

    const initialDb = parseFloat(initialValue);
    console.log(`[STEVO] Initial global volume: ${initialDb} dB`);

    // Normalise fader to middle then drag to a known position
    const fader = globalVol.locator(".fader-track");
    const box = await fader.boundingBox();
    expect(box).toBeTruthy();

    // Step A: Drag to ~50% to normalise from any extreme
    const startX2 = box!.x + box!.width * 0.5;
    const startY2 = box!.y + box!.height / 2;
    await page.mouse.move(startX2, startY2);
    await page.mouse.down();
    await page.waitForTimeout(350);
    await page.mouse.up();
    await page.waitForTimeout(500);

    // Step B: Drag from center to left in increments (decrease volume)
    await page.mouse.move(startX2, startY2);
    await page.mouse.down();
    await page.waitForTimeout(350);
    for (let i = 1; i <= 8; i++) {
      await page.mouse.move(
        box!.x + box!.width * (0.5 - 0.025 * i),
        startY2,
      );
      await page.waitForTimeout(50);
    }
    await page.mouse.up();
    await page.waitForTimeout(500);

    const afterDragValue = await dbDisplay.getAttribute("data-value");
    expect(afterDragValue).not.toBeNull();
    const afterDragDb = parseFloat(afterDragValue);
    console.log(`[STEVO] After drag global volume: ${afterDragDb} dB`);

    // Just verify value changed from initial
    expect(afterDragDb).not.toBeCloseTo(initialDb, 0);

    // Reload and verify
    await page.reload();
    await loginAs(page, "stevo");
    await page.goto("/stevo");

    await waitForMixer(page);

    const globalVolReloaded = page.locator(
      '[data-testid="global-volume-fader"]',
    );
    await expect(globalVolReloaded).toBeVisible({ timeout: 15000 });
    await page.waitForTimeout(1000);

    const dbDisplayReloaded = globalVolReloaded.locator(".db-display");
    const persistedValue = await dbDisplayReloaded.getAttribute("data-value");
    expect(persistedValue).not.toBeNull();
    const persistedDb = parseFloat(persistedValue);
    console.log(`[STEVO] After reload global volume: ${persistedDb} dB`);

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

    await waitForMixer(page);

    const globalVol = page.locator('[data-testid="global-volume-fader"]');
    await expect(globalVol).toBeVisible({ timeout: 5000 });

    // Wait for WebSocket to connect and receive initial state
    // If working correctly, value should be populated within 1s
    await page.waitForTimeout(1500);

    const dbDisplay = globalVol.locator(".db-display");
    const value = await dbDisplay.getAttribute("data-value");
    expect(value).not.toBeNull();

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
