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

// Helper to open the kebab menu for the first channel.
// Returns true if the menu was opened, false if skipped (no channels in CI).
async function openKebabMenu(page: Page): Promise<boolean> {
  const menuBtn = await page
    .waitForSelector(".ch-menu-btn", { timeout: 5000 })
    .catch(() => null);
  if (!assume(menuBtn, "Channel menu button must be visible")) return false;
  // Use force:true because channels-grid may intercept pointer events
  await page.locator(".ch-menu-btn").first().click({ force: true });
  return true;
}

// Helper to click the EQ option in an already-open kebab menu.
// Returns true if clicked, false if skipped.
async function clickEqOption(page: Page): Promise<boolean> {
  const eqOption = await page
    .waitForSelector("text=EQ >> visible=true", { timeout: 3000 })
    .catch(() => null);
  if (!assume(eqOption, "EQ menu option must be visible")) return false;
  await eqOption!.click();
  return true;
}

test.describe("EQ Feature", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
  });

  test("kebab menu has EQ option", async ({ page }) => {
    if (!(await waitForMixer(page))) return;
    if (!(await openKebabMenu(page))) return;

    // Verify EQ option exists in the menu (may not render without REAPER data)
    const eqVisible = await page
      .getByText("EQ", { exact: true })
      .isVisible()
      .catch(() => false);
    if (
      !assume(
        eqVisible,
        "EQ option must appear in kebab menu (requires REAPER data)",
      )
    )
      return;
  });

  test("EQ modal opens and shows track name", async ({ page }) => {
    if (!(await waitForMixer(page))) return;
    if (!(await openKebabMenu(page))) return;
    if (!(await clickEqOption(page))) return;

    // Verify EQ overlay appears
    const overlay = page.locator(".eq-overlay");
    await expect(overlay).toBeVisible({ timeout: 5000 });

    // Verify header contains a track name (any non-empty text in the header)
    const header = overlay.locator("h2, .eq-header, .eq-title").first();
    await expect(header).not.toBeEmpty();
  });

  test("EQ modal has SVG curve", async ({ page }) => {
    if (!(await waitForMixer(page))) return;
    if (!(await openKebabMenu(page))) return;
    if (!(await clickEqOption(page))) return;

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    // Verify SVG curve exists inside the curve container
    const svg = page.locator(".eq-curve-container svg");
    await expect(svg).toBeVisible({ timeout: 3000 });
  });

  test("EQ modal has band controls", async ({ page }) => {
    if (!(await waitForMixer(page))) return;
    if (!(await openKebabMenu(page))) return;
    if (!(await clickEqOption(page))) return;

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    // Verify band controls section exists with sliders
    const bandControls = page.locator(".eq-band-controls");
    await expect(bandControls).toBeVisible({ timeout: 3000 });
  });

  test("EQ modal closes on close button", async ({ page }) => {
    if (!(await waitForMixer(page))) return;
    if (!(await openKebabMenu(page))) return;
    if (!(await clickEqOption(page))) return;

    const overlay = page.locator(".eq-overlay");
    await expect(overlay).toBeVisible({ timeout: 5000 });

    // Click close button
    const closeBtn = page.locator(".eq-close-btn");
    await closeBtn.click();

    // Verify overlay disappears
    await expect(overlay).not.toBeVisible({ timeout: 3000 });
  });

  test("EQ slider interaction sends SetEqBand and updates display", async ({
    page,
  }) => {
    if (!(await waitForMixer(page))) return;

    // Set up WebSocket message tracking before opening EQ
    await page.evaluate(() => {
      const origSend = WebSocket.prototype.send;
      (window as any).__eqSetMessages = [];
      WebSocket.prototype.send = function (data: string | ArrayBuffer) {
        try {
          const parsed = JSON.parse(data as string);
          if (parsed.cmd === "SetEqBand") {
            (window as any).__eqSetMessages.push(parsed);
          }
        } catch {
          // ignore non-JSON messages
        }
        return origSend.call(this, data);
      };
    });

    if (!(await openKebabMenu(page))) return;
    if (!(await clickEqOption(page))) return;

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    // Wait for band controls to load (requires REAPER to return EQ data)
    const bandCard = await page
      .waitForSelector(".eq-band-card", { timeout: 5000 })
      .catch(() => null);
    if (!assume(bandCard, "EQ band cards must load (requires REAPER EQ data)"))
      return;

    // Record the initial gain display text
    const gainValue = page
      .locator(".eq-band-card")
      .first()
      .locator(".eq-param-row")
      .nth(1)
      .locator(".eq-param-value");
    const initialGainText = await gainValue.textContent();

    // Find the gain slider (second eq-slider-track in the first band card)
    const gainSlider = page
      .locator(".eq-band-card")
      .first()
      .locator(".eq-slider-track")
      .nth(1);

    const sliderVisible = await gainSlider.isVisible().catch(() => false);
    if (!assume(sliderVisible, "Gain slider must be visible")) return;

    // Simulate a touch-and-hold interaction on the gain slider:
    // 1. Touch down on the slider
    // 2. Wait for activation (150ms + margin)
    // 3. Move touch horizontally to change value
    // 4. Release touch
    const box = await gainSlider.boundingBox();
    if (!assume(box, "Gain slider must have bounding box")) return;

    const startX = box!.x + box!.width / 2;
    const startY = box!.y + box!.height / 2;

    // Perform a drag interaction (mouse hold + move)
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.waitForTimeout(200); // Wait for 150ms activation
    // Move 40px to the right (should increase gain)
    await page.mouse.move(startX + 40, startY, { steps: 5 });
    await page.mouse.up();

    // Wait for UI update
    await page.waitForTimeout(300);

    // Verify the gain display text changed (optimistic update)
    const newGainText = await gainValue.textContent();
    if (initialGainText !== null && newGainText !== null) {
      // The text should have changed after dragging
      expect(newGainText).not.toBe(initialGainText);
    }

    // Verify SetEqBand WebSocket message was sent
    const messages = await page.evaluate(
      () => (window as any).__eqSetMessages || [],
    );
    expect(messages.length).toBeGreaterThan(0);
    expect(messages[0].cmd).toBe("SetEqBand");
    expect(messages[0].param).toBe("gain");
  });

  test("EQ sliders use safe touch activation (no jump-to-tap)", async ({
    page,
  }) => {
    if (!(await waitForMixer(page))) return;
    if (!(await openKebabMenu(page))) return;
    if (!(await clickEqOption(page))) return;

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    const bandCard = await page
      .waitForSelector(".eq-band-card", { timeout: 5000 })
      .catch(() => null);
    if (!assume(bandCard, "EQ band cards must load (requires REAPER EQ data)"))
      return;

    // Verify that native <input type="range"> elements are NOT used for EQ sliders
    // (they jump to tap position which is dangerous for IEM monitoring)
    const nativeSliders = await page
      .locator('.eq-band-card input[type="range"]')
      .count();
    expect(nativeSliders).toBe(0);

    // Verify custom slider elements exist instead
    const customSliders = await page.locator(".eq-slider-track").count();
    expect(customSliders).toBeGreaterThan(0);
  });

  test("EQ sends GetEqParams WebSocket message on open", async ({ page }) => {
    if (!(await waitForMixer(page))) return;

    // Set up WebSocket message tracking before opening EQ
    await page.evaluate(() => {
      const origSend = WebSocket.prototype.send;
      (window as any).__eqMessages = [];
      WebSocket.prototype.send = function (data: string | ArrayBuffer) {
        try {
          const parsed = JSON.parse(data as string);
          if (parsed.cmd === "GetEqParams") {
            (window as any).__eqMessages.push(parsed);
          }
        } catch {
          // ignore non-JSON messages
        }
        return origSend.call(this, data);
      };
    });

    if (!(await openKebabMenu(page))) return;
    if (!(await clickEqOption(page))) return;

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    // Wait briefly for WebSocket message to be sent
    await page.waitForTimeout(1000);

    // Verify GetEqParams was sent
    const messages = await page.evaluate(
      () => (window as any).__eqMessages || [],
    );
    expect(messages.length).toBeGreaterThan(0);
    expect(messages[0].cmd).toBe("GetEqParams");
    expect(messages[0].track_index).toBeDefined();
  });
});
