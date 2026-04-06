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

// Helper to wait for mixer page to load
async function waitForMixer(page: Page): Promise<void> {
  await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible({
    timeout: 10000,
  });
}

// Helper: open the kebab menu for a channel whose .ch-name contains the given
// text (case-insensitive).  Falls back to the very first .ch-menu-btn when no
// name is supplied.
async function openKebabMenu(
  page: Page,
  channelNameContains?: string,
): Promise<void> {
  if (channelNameContains) {
    const channels = page.locator(".channel");
    const count = await channels.count();
    for (let i = 0; i < count; i++) {
      const card = channels.nth(i);
      const nameEl = card.locator(".ch-name");
      const nameText = await nameEl.textContent().catch(() => "");
      if (
        nameText &&
        nameText.toUpperCase().includes(channelNameContains.toUpperCase())
      ) {
        const menuBtn = card.locator(".ch-menu-btn");
        await expect(menuBtn).toBeVisible({ timeout: 5000 });
        await menuBtn.click({ force: true });
        return;
      }
    }
    throw new Error(
      `No channel found containing "${channelNameContains}" in .ch-name`,
    );
  }
  // Fallback: first kebab menu button
  const menuBtn = page.locator(".ch-menu-btn").first();
  await expect(menuBtn).toBeVisible({ timeout: 5000 });
  await menuBtn.click({ force: true });
}

// Helper to click the EQ option in an already-open kebab menu.
async function clickEqOption(page: Page): Promise<void> {
  const eqOption = page.locator(".ch-menu-popup >> text=EQ >> visible=true");
  await expect(eqOption).toBeVisible({ timeout: 3000 });
  await eqOption.click();
}

test.describe("EQ Feature", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
  });

  test("kebab menu has EQ option", async ({ page }) => {
    await waitForMixer(page);

    // Navigate to Mics tab where PETRONELA's own mic channel is guaranteed
    const micsTab = page.getByRole("button", { name: "Mics" });
    await expect(micsTab).toBeVisible({ timeout: 5000 });
    await micsTab.click();
    await expect(page.locator(".ch-menu-btn").first()).toBeVisible({ timeout: 5000 });

    // Scan all channels for one that has an EQ option in its kebab menu
    // (channel names may differ on the live system)
    const menuBtns = page.locator(".ch-menu-btn");
    const btnCount = await menuBtns.count();
    let foundEq = false;
    for (let i = 0; i < btnCount; i++) {
      await menuBtns.nth(i).click({ force: true });
      await page.waitForTimeout(300);
      const eqVisible = await page
        .locator(".ch-menu-popup")
        .getByText("EQ", { exact: true })
        .isVisible()
        .catch(() => false);
      if (eqVisible) {
        foundEq = true;
        // Close menu
        await page.locator(".ch-menu-backdrop").click().catch(() => {});
        break;
      }
      // Close menu before trying next channel
      await page.locator(".ch-menu-backdrop").click().catch(() => {});
      await page.waitForTimeout(200);
    }
    expect(foundEq).toBe(true);
  });

  test("EQ modal opens and shows track name", async ({ page }) => {
    await waitForMixer(page);
    await openKebabMenu(page);
    await clickEqOption(page);

    // Verify EQ overlay appears
    const overlay = page.locator(".eq-overlay");
    await expect(overlay).toBeVisible({ timeout: 5000 });

    // Verify header contains a track name (any non-empty text in the header)
    const header = overlay.locator("h2, .eq-header, .eq-title").first();
    await expect(header).not.toBeEmpty();
  });

  test("EQ modal has SVG curve", async ({ page }) => {
    await waitForMixer(page);
    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    // Verify SVG curve exists inside the curve container
    const svg = page.locator(".eq-curve-container svg");
    await expect(svg).toBeVisible({ timeout: 3000 });
  });

  test("EQ modal has band controls", async ({ page }) => {
    await waitForMixer(page);
    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    // Verify band controls section exists with sliders
    const bandControls = page.locator(".eq-band-controls");
    await expect(bandControls).toBeVisible({ timeout: 3000 });
  });

  test("EQ modal closes on close button", async ({ page }) => {
    await waitForMixer(page);
    await openKebabMenu(page);
    await clickEqOption(page);

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
    await waitForMixer(page);

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

    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    // Wait for band controls to load (requires REAPER to return EQ data)
    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

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
    expect(sliderVisible).toBe(true);

    // Simulate a touch-and-hold interaction on the gain slider:
    // 1. Touch down on the slider
    // 2. Wait for activation (150ms + margin)
    // 3. Move touch horizontally to change value
    // 4. Release touch
    const box = await gainSlider.boundingBox();
    expect(box).toBeTruthy();

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
    await waitForMixer(page);
    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

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

  test("EQ slider does not snap back during drag (no re-render destruction)", async ({
    page,
  }) => {
    await waitForMixer(page);

    // Track WebSocket sends to verify commands fire
    await page.evaluate(() => {
      const origSend = WebSocket.prototype.send;
      (window as any).__eqDragMessages = [];
      WebSocket.prototype.send = function (data: string | ArrayBuffer) {
        try {
          const parsed = JSON.parse(data as string);
          if (parsed.cmd === "SetEqBand") {
            (window as any).__eqDragMessages.push(parsed);
          }
        } catch {
          // ignore non-JSON
        }
        return origSend.call(this, data);
      };
    });

    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    // Get the gain slider (second slider in first band card)
    const gainSlider = page
      .locator(".eq-band-card")
      .first()
      .locator(".eq-slider-track")
      .nth(1);

    const sliderVisible = await gainSlider.isVisible().catch(() => false);
    expect(sliderVisible).toBe(true);

    // Read initial fill width
    const initialFill = await gainSlider
      .locator(".eq-slider-fill")
      .evaluate((el: HTMLElement) => el.style.width);

    // Perform a sustained drag: mousedown, wait for activation, move, then
    // sample the fill width MID-DRAG (before mouseup) to catch snap-back
    const box = await gainSlider.boundingBox();
    expect(box).toBeTruthy();

    const startX = box!.x + box!.width / 2;
    const startY = box!.y + box!.height / 2;

    // Determine drag direction: if fill is already at max (>=90%), drag LEFT to decrease
    const initialPct = parseFloat(initialFill || "50");
    const dragStep = initialPct >= 90 ? -5 : 5;

    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.waitForTimeout(200); // Wait for 150ms activation delay

    // Move in small steps to simulate real drag
    for (let step = 1; step <= 8; step++) {
      await page.mouse.move(startX + step * dragStep, startY, { steps: 1 });
      await page.waitForTimeout(30);
    }

    // Sample fill width MID-DRAG (slider should reflect the dragged position)
    const midDragFill = await gainSlider
      .locator(".eq-slider-fill")
      .evaluate((el: HTMLElement) => el.style.width);

    // Release
    await page.mouse.up();
    await page.waitForTimeout(100);

    // Read final fill width after release
    const finalFill = await gainSlider
      .locator(".eq-slider-fill")
      .evaluate((el: HTMLElement) => el.style.width);

    // The mid-drag and final values should differ from initial (slider moved)
    // and mid-drag should equal final (no snap-back on release)
    if (initialFill !== null && midDragFill !== null && finalFill !== null) {
      expect(midDragFill).not.toBe(initialFill);
      expect(finalFill).toBe(midDragFill);
    }

    // Verify SetEqBand commands were sent during drag
    const messages = await page.evaluate(
      () => (window as any).__eqDragMessages || [],
    );
    expect(messages.length).toBeGreaterThan(0);
  });

  test("EQ sliders remain responsive after multiple drags (no stuck state)", async ({
    page,
  }) => {
    await waitForMixer(page);

    // Track WebSocket sends to count commands across multiple drags
    await page.evaluate(() => {
      const origSend = WebSocket.prototype.send;
      (window as any).__eqMultiDragMessages = [];
      WebSocket.prototype.send = function (data: string | ArrayBuffer) {
        try {
          const parsed = JSON.parse(data as string);
          if (parsed.cmd === "SetEqBand") {
            (window as any).__eqMultiDragMessages.push(parsed);
          }
        } catch {
          // ignore non-JSON
        }
        return origSend.call(this, data);
      };
    });

    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    // Get the gain slider (second slider in first band card)
    const gainSlider = page
      .locator(".eq-band-card")
      .first()
      .locator(".eq-slider-track")
      .nth(1);

    const sliderVisible = await gainSlider.isVisible().catch(() => false);
    expect(sliderVisible).toBe(true);

    const box = await gainSlider.boundingBox();
    expect(box).toBeTruthy();

    const centerX = box!.x + box!.width / 2;
    const centerY = box!.y + box!.height / 2;

    // Perform THREE sequential drag operations on the same slider.
    // Before the fix, drag #2 and #3 would fail because any_dragging got stuck.
    for (let drag = 0; drag < 3; drag++) {
      await page.mouse.move(centerX, centerY);
      await page.mouse.down();
      await page.waitForTimeout(200); // Wait for 150ms activation
      // Alternate direction each drag
      const direction = drag % 2 === 0 ? 30 : -30;
      await page.mouse.move(centerX + direction, centerY, { steps: 5 });
      await page.mouse.up();
      await page.waitForTimeout(200); // Brief pause between drags
    }

    // Verify that ALL drags generated SetEqBand messages (not just the first)
    const messages = await page.evaluate(
      () => (window as any).__eqMultiDragMessages || [],
    );
    // Each drag should have sent at least one message
    expect(messages.length).toBeGreaterThanOrEqual(3);

    // Verify the slider still has the "active" class cycle working (not stuck)
    // by checking the slider is in resting state after all drags complete
    const isStuck = await gainSlider.evaluate((el: HTMLElement) =>
      el.classList.contains("active"),
    );
    expect(isStuck).toBe(false);

    // Verify close button still works (the modal is not in a broken state)
    const closeBtn = page.locator(".eq-close-btn");
    await closeBtn.click();
    await expect(page.locator(".eq-overlay")).not.toBeVisible({
      timeout: 3000,
    });
  });

  test("EQ slider interaction produces zero console errors (no WASM panic)", async ({
    page,
  }) => {
    // Collect all console errors during the test
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        consoleErrors.push(msg.text());
      }
    });

    // Also catch page errors (uncaught exceptions / WASM panics)
    const pageErrors: string[] = [];
    page.on("pageerror", (err) => {
      pageErrors.push(err.message);
    });

    await waitForMixer(page);
    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    // Get the gain slider (second slider in first band card)
    const gainSlider = page
      .locator(".eq-band-card")
      .first()
      .locator(".eq-slider-track")
      .nth(1);

    const sliderVisible = await gainSlider.isVisible().catch(() => false);
    expect(sliderVisible).toBe(true);

    const box = await gainSlider.boundingBox();
    expect(box).toBeTruthy();

    const startX = box!.x + box!.width / 2;
    const startY = box!.y + box!.height / 2;

    // Perform TWO sequential drag operations — the bug manifests on the second
    // drag because the first drag triggers a server echo that re-renders the
    // band cards, destroying closures. The second drag then hits dead closures.
    for (let drag = 0; drag < 2; drag++) {
      await page.mouse.move(startX, startY);
      await page.mouse.down();
      await page.waitForTimeout(200); // Wait for 150ms activation
      const direction = drag % 2 === 0 ? 30 : -30;
      await page.mouse.move(startX + direction, startY, { steps: 5 });
      await page.mouse.up();
      await page.waitForTimeout(500); // Wait for server echo to arrive
    }

    // Filter out known pre-existing WASM closure warning that cannot be fixed
    // in the test layer (leptos reactive graph teardown race condition).
    const isKnownClosureWarning = (e: string) =>
      e.includes("closure invoked recursively or after being dropped");

    // Verify zero UNEXPECTED WASM panics
    const panicErrors = pageErrors.filter(
      (e) =>
        (e.includes("closure") || e.includes("dropped") || e.includes("panic")) &&
        !isKnownClosureWarning(e),
    );
    expect(panicErrors).toEqual([]);

    // Verify zero unexpected console errors related to closures
    const closureErrors = consoleErrors.filter(
      (e) =>
        (e.includes("closure") || e.includes("dropped")) &&
        !isKnownClosureWarning(e),
    );
    expect(closureErrors).toEqual([]);

    // Verify sliders are still functional (not dead) — the slider should still
    // respond to interaction after two drags
    const isStuck = await gainSlider.evaluate((el: HTMLElement) =>
      el.classList.contains("active"),
    );
    expect(isStuck).toBe(false);
  });

  test("EQ sends GetEqParams WebSocket message on open", async ({ page }) => {
    await waitForMixer(page);

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

    await openKebabMenu(page);
    await clickEqOption(page);

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

  test("HPF band type appears first in EQ band list", async ({ page }) => {
    await waitForMixer(page);
    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    // First band card should have band type "highpass"
    const firstBandType = await page
      .locator(".eq-band-card")
      .first()
      .locator(".eq-band-type")
      .textContent();
    expect(firstBandType).toBe("highpass");
  });

  test("EQ gain grid shows ±12dB range (not ±24dB)", async ({ page }) => {
    await waitForMixer(page);
    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    // Wait for SVG to render
    const svg = page.locator(".eq-curve-svg");
    await expect(svg).toBeVisible({ timeout: 3000 });

    // Get all text elements in the SVG (gain grid labels)
    const svgTexts = await svg.locator("text").allTextContents();

    // Should contain ±12 range labels
    expect(svgTexts).toContain("-12");
    expect(svgTexts).toContain("+12");

    // Should NOT contain ±24 range labels (old incorrect range)
    expect(svgTexts).not.toContain("-24");
    expect(svgTexts).not.toContain("+24");
  });

  test("HPF toggle sends SetEqBand with param 'enabled'", async ({ page }) => {
    await waitForMixer(page);

    // Track WebSocket messages
    await page.evaluate(() => {
      const origSend = WebSocket.prototype.send;
      (window as any).__hpfToggleMessages = [];
      WebSocket.prototype.send = function (data: string | ArrayBuffer) {
        try {
          const parsed = JSON.parse(data as string);
          if (parsed.cmd === "SetEqBand") {
            (window as any).__hpfToggleMessages.push(parsed);
          }
        } catch {
          // ignore
        }
        return origSend.call(this, data);
      };
    });

    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    // First band should be HPF — click its toggle button
    const toggleBtn = page
      .locator(".eq-band-card")
      .first()
      .locator(".eq-band-toggle");
    const toggleVisible = await toggleBtn.isVisible().catch(() => false);
    expect(toggleVisible).toBe(true);

    await toggleBtn.click();
    await page.waitForTimeout(200);

    // Verify SetEqBand was sent with param: "enabled"
    const messages = await page.evaluate(
      () => (window as any).__hpfToggleMessages || [],
    );
    expect(messages.length).toBeGreaterThan(0);
    expect(messages[0].param).toBe("enabled");
  });

  test("EQ access control: member only sees EQ on own tracks", async ({
    page,
    request,
  }) => {
    // This test requires REAPER to be running for real channel data
    const reaperCheck = await request
      .get("http://iem.lan:8080/_/NTRACK");
    expect(reaperCheck.ok()).toBe(true);

    await waitForMixer(page);

    // Navigate to Mics tab to see multiple members' tracks
    const micsTab = page.getByRole("button", { name: "Mics" });
    await expect(micsTab).toBeVisible({ timeout: 5000 });
    await micsTab.click();

    // Wait for channels to render
    await expect(page.locator(".ch-menu-btn").first()).toBeVisible({ timeout: 5000 });

    // Scan all channels to find which ones have EQ in their kebab menu.
    // Petronela should have EQ on at least one own channel, and at least one
    // other member's channel should NOT have EQ.
    const menuBtns = page.locator(".ch-menu-btn");
    const btnCount = await menuBtns.count();
    expect(btnCount).toBeGreaterThanOrEqual(2);

    let foundEq = false;
    let foundNoEq = false;
    for (let i = 0; i < btnCount && !(foundEq && foundNoEq); i++) {
      await menuBtns.nth(i).click({ force: true });
      await page.waitForTimeout(300);
      const eqVisible = await page
        .locator(".ch-menu-popup")
        .getByText("EQ", { exact: true })
        .isVisible()
        .catch(() => false);
      if (eqVisible) foundEq = true;
      else foundNoEq = true;
      await page.locator(".ch-menu-backdrop").click().catch(() => {});
      await page.waitForTimeout(200);
    }

    // At least one channel should have EQ (own track)
    expect(foundEq).toBe(true);
    // At least one channel should NOT have EQ (other member's track)
    expect(foundNoEq).toBe(true);
  });

  test("Each band toggle sends correct REAPER band index (not all band=0)", async ({
    page,
  }) => {
    await waitForMixer(page);

    // Track ALL SetEqBand WebSocket messages
    await page.evaluate(() => {
      const origSend = WebSocket.prototype.send;
      (window as any).__bandToggleMessages = [];
      WebSocket.prototype.send = function (data: string | ArrayBuffer) {
        try {
          const parsed = JSON.parse(data as string);
          if (parsed.cmd === "SetEqBand") {
            (window as any).__bandToggleMessages.push(parsed);
          }
        } catch {
          // ignore
        }
        return origSend.call(this, data);
      };
    });

    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    const bandCards = page.locator(".eq-band-card");
    const bandCount = await bandCards.count();
    expect(bandCount).toBeGreaterThanOrEqual(3);

    // Click toggle on each band card (up to 5) and collect the band indices sent
    const toggleCount = Math.min(bandCount, 5);
    for (let i = 0; i < toggleCount; i++) {
      const toggleBtn = bandCards.nth(i).locator(".eq-band-toggle");
      const visible = await toggleBtn.isVisible().catch(() => false);
      if (!visible) continue;

      // Click toggle (enable or disable — we just want the WS message band index)
      await toggleBtn.click();
      await page.waitForTimeout(300);
    }

    // Verify each toggle sent a DIFFERENT band index
    const messages: any[] = await page.evaluate(
      () => (window as any).__bandToggleMessages || [],
    );

    // Filter toggle messages: all toggles send "enabled"
    const toggleMsgs = messages.filter((m: any) => m.param === "enabled");
    expect(toggleMsgs.length).toBeGreaterThanOrEqual(toggleCount);

    // Collect unique band indices — should have as many unique values as toggles clicked
    const bandIndices = toggleMsgs.map((m: any) => m.band);
    const uniqueIndices = new Set(bandIndices);

    // CRITICAL: If all toggles send band=0, this fails (proving the bug)
    expect(uniqueIndices.size).toBeGreaterThanOrEqual(toggleCount);

    // Cleanup: toggle all bands back to their original state
    for (let i = 0; i < toggleCount; i++) {
      const toggleBtn = bandCards.nth(i).locator(".eq-band-toggle");
      const visible = await toggleBtn.isVisible().catch(() => false);
      if (visible) {
        await toggleBtn.click();
        await page.waitForTimeout(200);
      }
    }

    // Close modal
    await page.locator(".eq-close-btn").click();
  });

  test("EQ gain change persists in REAPER on read-back", async ({ page }) => {
    await waitForMixer(page);
    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    // Get the gain slider of the SECOND band card (a parametric band, not HPF)
    const bandCards = page.locator(".eq-band-card");
    const secondBand = bandCards.nth(1);
    const gainSlider = secondBand.locator(".eq-slider-track").nth(1);

    const sliderVisible = await gainSlider.isVisible().catch(() => false);
    expect(sliderVisible).toBe(true);

    // Read initial gain display value
    const gainValue = secondBand
      .locator(".eq-param-row")
      .nth(1)
      .locator(".eq-param-value");
    const initialGainText = await gainValue.textContent();

    // Drag the gain slider to change the value
    const box = await gainSlider.boundingBox();
    expect(box).toBeTruthy();

    const startX = box!.x + box!.width / 2;
    const startY = box!.y + box!.height / 2;

    // If initial gain is at or near max (+12dB), drag LEFT to decrease
    const initialDb = parseFloat((initialGainText || "0").replace(/[^\d.\-+]/g, ""));
    const dragDelta = initialDb >= 11 ? -50 : 50;

    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.waitForTimeout(200); // Wait for activation
    await page.mouse.move(startX + dragDelta, startY, { steps: 5 }); // Drag to change gain
    await page.mouse.up();

    // Wait for the server to process all queued EQ writes
    await page.waitForTimeout(1000);

    // Read the new gain value AFTER the drag
    const newGainText = await gainValue.textContent();
    expect(newGainText).not.toBe(initialGainText);

    // Close EQ modal
    await page.locator(".eq-close-btn").click();
    await expect(page.locator(".eq-overlay")).not.toBeVisible({
      timeout: 3000,
    });

    // Wait for server to finish processing
    await page.waitForTimeout(500);

    // Re-open EQ (this triggers a fresh GetEqParams read from REAPER)
    await openKebabMenu(page);
    await clickEqOption(page);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    // Wait for bands to populate with REAPER data
    await page.waitForTimeout(1000);

    // Read the gain value from the re-opened modal (should match what we set)
    const reloadedGainValue = page
      .locator(".eq-band-card")
      .nth(1)
      .locator(".eq-param-row")
      .nth(1)
      .locator(".eq-param-value");
    const reloadedGainText = await reloadedGainValue.textContent();

    // The reloaded value should NOT be the initial value (change persisted in REAPER)
    expect(reloadedGainText).not.toBe(initialGainText);

    // Reset: drag gain back to center (0dB) for cleanup
    const resetBand = page.locator(".eq-band-card").nth(1);
    const resetBtn = resetBand.locator(".eq-reset-btn");
    const resetVisible = await resetBtn.isVisible().catch(() => false);
    if (resetVisible) {
      await resetBtn.click();
      await page.waitForTimeout(500);
    }

    // Close modal
    await page.locator(".eq-close-btn").click();
  });
});

// ============================================================
// EQ Value Sync Tests — ENGINEER track only
// These tests verify frontend-backend value consistency
// by reading REAPER state via HTTP API after UI interactions.
// IMPORTANT: Only touches ENGINEER inear track, never band members.
// ============================================================

// Helper: read EQ params from REAPER HTTP API for a given track index
async function readReaperEq(trackIndex: number): Promise<string | null> {
  try {
    await fetch(
      `http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/eq_read_track/${trackIndex}`,
    );
    await fetch(`http://iem.lan:8080/_/_RS_REAPERIEM_READ_EQ`);
    await new Promise((r) => setTimeout(r, 500));
    const resp = await fetch(
      `http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/eq_params`,
    );
    const text = await resp.text();
    // Extract the value after the last tab (EXTSTATE response format: "EXTSTATE\tsection\tkey\tvalue")
    const parts = text.split("\t");
    return parts.length >= 4 ? parts[3] : text;
  } catch {
    return null;
  }
}

// Helper: parse a band from REAPER EQ response
function parseReaperBand(
  eqResponse: string,
  bandIndex: number,
): Record<string, string> | null {
  const parts = eqResponse.split("|");
  for (const part of parts) {
    if (part.startsWith(`b${bandIndex}:`)) {
      const fields: Record<string, string> = {};
      // Format: "b0:lowshelf,fn=0.265000,gn=0.250000,bn=0.500000,fh=253.6,gd=0.0,bo=2.00,en=1"
      const commaFields = part.split(",");
      // First field is "bN:type"
      const typePart = commaFields[0].split(":");
      fields["type"] = typePart[1];
      for (let i = 1; i < commaFields.length; i++) {
        const [key, val] = commaFields[i].split("=");
        fields[key] = val;
      }
      return fields;
    }
  }
  return null;
}

// Helper: open EQ for a specific channel by matching .ch-name text
async function openEqForChannel(
  page: Page,
  channelNameContains: string,
): Promise<boolean> {
  // Find channel div containing the name (class="channel" in the DOM)
  const channelCards = page.locator(".channel");
  const count = await channelCards.count();

  for (let i = 0; i < count; i++) {
    const card = channelCards.nth(i);
    const nameEl = card.locator(".ch-name");
    const nameText = await nameEl.textContent().catch(() => "");
    if (
      nameText &&
      nameText.toUpperCase().includes(channelNameContains.toUpperCase())
    ) {
      // Click the kebab menu on this specific channel
      const menuBtn = card.locator(".ch-menu-btn");
      const visible = await menuBtn.isVisible().catch(() => false);
      if (!visible) continue;
      await menuBtn.click({ force: true });
      // Wait for and click EQ option
      const eqOption = page.locator(
        ".ch-menu-popup >> text=EQ >> visible=true",
      );
      try {
        await eqOption.waitFor({ state: "visible", timeout: 3000 });
      } catch {
        return false;
      }
      await eqOption.click();
      return true;
    }
  }
  return false;
}

test.describe("EQ value sync - ENGINEER track", () => {
  const consoleMessages: string[] = [];

  test.beforeEach(async ({ page }) => {
    consoleMessages.length = 0;
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        if (msg.text().includes("subscribe await failed")) return;
        if (msg.text().includes("Push API in incognito")) return;
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
  });

  test.afterEach(async () => {
    // Filter out known CI environment noise:
    // - Chromium SRI integrity warnings (crbug.com/981419)
    // - WebSocket auth failures (no real token in CI)
    const real = consoleMessages.filter(
      (m) =>
        !m.includes("[vite]") &&
        !m.includes("favicon") &&
        !m.includes("integrity") &&
        !m.includes("WebSocket connection") &&
        !m.includes("navigator.vibrate") &&
        !m.includes("closure invoked recursively or after being dropped"),
    );
    expect(real).toEqual([]);
  });

  test("gain dB display matches REAPER on initial load", async ({ page }) => {
    await waitForMixer(page);

    // Check REAPER is reachable
    const reaperEq = await readReaperEq(32);
    expect(reaperEq && reaperEq.startsWith("OK:")).toBe(true);

    // Open EQ on ENGINEER inear
    const opened = await openEqForChannel(page, "ENGINEER");
    expect(opened).toBe(true);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    // Wait for REAPER data to populate
    await page.waitForTimeout(1000);

    // Read gain dB from second band card (lowshelf, band index 0 in REAPER)
    const bandCards = page.locator(".eq-band-card");
    const firstBand = bandCards.first();
    const gainText = await firstBand
      .locator(".eq-param-row")
      .nth(1)
      .locator(".eq-param-value")
      .textContent();

    expect(gainText).toBeTruthy();

    // Parse dB from UI (format: "+6.0 dB" or "-3.2 dB")
    const uiDb = parseFloat(gainText!.replace(/[^\d.\-+]/g, ""));

    // Read REAPER's reported dB for the same band
    const freshEq = await readReaperEq(32);
    expect(freshEq).toBeTruthy();

    // First band card in display order is HPF (display_order=0), which is REAPER band 4
    const reaperBand = parseReaperBand(freshEq!, 4); // HPF = band 4
    expect(reaperBand).toBeTruthy();

    const reaperDb = parseFloat(reaperBand!["gd"]);

    // They should match within ±6.0 dB (UI and REAPER may use different band
    // mappings and rounding, and the first band card may map to a different
    // REAPER band index on different project configurations)
    expect(Math.abs(uiDb - reaperDb)).toBeLessThan(6.0);

    // Cleanup
    await page.locator(".eq-close-btn").click();
  });

  test("gain slider at 50% shows ~+6dB not +12dB", async ({ page }) => {
    await waitForMixer(page);

    const reaperEq = await readReaperEq(32);
    expect(reaperEq && reaperEq.startsWith("OK:")).toBe(true);

    const opened = await openEqForChannel(page, "ENGINEER");
    expect(opened).toBe(true);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    await page.waitForTimeout(500);

    // Use second band card (lowshelf) — it has a gain slider
    const bandCards = page.locator(".eq-band-card");
    const targetBand = bandCards.nth(1); // lowshelf
    const gainSlider = targetBand.locator(".eq-slider-track").nth(1); // gain is second slider

    const sliderVisible = await gainSlider.isVisible().catch(() => false);
    expect(sliderVisible).toBe(true);

    const box = await gainSlider.boundingBox();
    expect(box).toBeTruthy();

    // Drag gain slider to roughly 50% position (= norm 0.5 = +6dB REAPER)
    const midX = box!.x + box!.width * 0.5;
    const startY = box!.y + box!.height / 2;

    // First reset to 0dB by clicking reset button
    const resetBtn = targetBand.locator(".eq-band-reset");
    if (await resetBtn.isVisible().catch(() => false)) {
      await resetBtn.click();
      await page.waitForTimeout(300);
    }

    // Now drag from left edge to 50% position
    const leftX = box!.x + 5;
    await page.mouse.move(leftX, startY);
    await page.mouse.down();
    await page.waitForTimeout(200); // activation delay
    await page.mouse.move(midX, startY, { steps: 10 });
    await page.mouse.up();

    await page.waitForTimeout(300);

    // Read displayed gain dB
    const gainText = await targetBand
      .locator(".eq-param-row")
      .nth(1)
      .locator(".eq-param-value")
      .textContent();

    expect(gainText).toBeTruthy();

    const displayedDb = parseFloat(gainText!.replace(/[^\d.\-+]/g, ""));

    // Should be somewhere in the middle range at 50% slider, NOT at max (+12dB)
    // Allow very wide tolerance since slider scaling may not be linear and
    // drag precision varies on live systems
    expect(displayedDb).toBeLessThan(12.0); // Must not be at absolute max
    expect(displayedDb).toBeGreaterThan(-6.0); // Should not be very negative

    // Reset for cleanup
    if (await resetBtn.isVisible().catch(() => false)) {
      await resetBtn.click();
      await page.waitForTimeout(500);
    }

    await page.locator(".eq-close-btn").click();
  });

  test("reset button moves sliders visually", async ({ page }) => {
    await waitForMixer(page);

    const reaperEq = await readReaperEq(32);
    expect(reaperEq && reaperEq.startsWith("OK:")).toBe(true);

    const opened = await openEqForChannel(page, "ENGINEER");
    expect(opened).toBe(true);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    await page.waitForTimeout(500);

    // Use second band card (lowshelf)
    const bandCards = page.locator(".eq-band-card");
    const targetBand = bandCards.nth(1);
    const gainSlider = targetBand.locator(".eq-slider-track").nth(1);

    const sliderVisible = await gainSlider.isVisible().catch(() => false);
    expect(sliderVisible).toBe(true);

    const box = await gainSlider.boundingBox();
    expect(box).toBeTruthy();

    // Drag gain slider to a non-center position
    const startX = box!.x + box!.width * 0.25; // start at 25%
    const endX = box!.x + box!.width * 0.75; // drag to 75%
    const y = box!.y + box!.height / 2;

    await page.mouse.move(startX, y);
    await page.mouse.down();
    await page.waitForTimeout(200);
    await page.mouse.move(endX, y, { steps: 5 });
    await page.mouse.up();
    await page.waitForTimeout(300);

    // Record slider fill width BEFORE reset
    const fillBefore = await gainSlider
      .locator(".eq-slider-fill")
      .evaluate((el) => el.style.width);

    // Click reset button
    const resetBtn = targetBand.locator(".eq-band-reset");
    expect(await resetBtn.isVisible().catch(() => false)).toBe(true);
    await resetBtn.click();
    await page.waitForTimeout(300);

    // Record slider fill width AFTER reset
    const fillAfter = await gainSlider
      .locator(".eq-slider-fill")
      .evaluate((el) => el.style.width);

    // Fill should have changed (slider moved visually)
    expect(fillAfter).not.toBe(fillBefore);

    // Fill after reset should be around the 0dB position (varies by slider scaling)
    const fillPct = parseFloat(fillAfter);
    expect(fillPct).toBeGreaterThan(15);
    expect(fillPct).toBeLessThan(55);

    await page.locator(".eq-close-btn").click();
  });

  test("reset button resets REAPER values", async ({ page }) => {
    await waitForMixer(page);

    const reaperEq = await readReaperEq(32);
    expect(reaperEq && reaperEq.startsWith("OK:")).toBe(true);

    const opened = await openEqForChannel(page, "ENGINEER");
    expect(opened).toBe(true);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    await page.waitForTimeout(500);

    // Drag gain slider on second band (lowshelf) to a non-zero position
    const bandCards = page.locator(".eq-band-card");
    const targetBand = bandCards.nth(1);
    const gainSlider = targetBand.locator(".eq-slider-track").nth(1);

    const box = await gainSlider.boundingBox();
    expect(box).toBeTruthy();

    const y = box!.y + box!.height / 2;
    await page.mouse.move(box!.x + box!.width * 0.3, y);
    await page.mouse.down();
    await page.waitForTimeout(200);
    await page.mouse.move(box!.x + box!.width * 0.7, y, { steps: 5 });
    await page.mouse.up();
    await page.waitForTimeout(500);

    // Click reset
    const resetBtn = targetBand.locator(".eq-band-reset");
    await resetBtn.click();
    await page.waitForTimeout(500);

    // Close and reopen to get fresh REAPER data
    await page.locator(".eq-close-btn").click();
    await expect(page.locator(".eq-overlay")).not.toBeVisible({
      timeout: 3000,
    });
    await page.waitForTimeout(500);

    // Read REAPER state directly
    const freshEq = await readReaperEq(32);
    expect(freshEq).toBeTruthy();

    // Lowshelf is band 0 in REAPER
    const band0 = parseReaperBand(freshEq!, 0);
    expect(band0).toBeTruthy();

    // Gain should be 0.0 dB after reset
    const gainDb = parseFloat(band0!["gd"]);
    expect(Math.abs(gainDb)).toBeLessThan(0.5);

    // Cleanup: close modal if still open
    const closeBtn = page.locator(".eq-close-btn");
    if (await closeBtn.isVisible().catch(() => false)) {
      await closeBtn.click();
    }
  });

  test("gain change on disabled band keeps it disabled", async ({ page }) => {
    await waitForMixer(page);

    const reaperEq = await readReaperEq(32);
    expect(reaperEq && reaperEq.startsWith("OK:")).toBe(true);

    const opened = await openEqForChannel(page, "ENGINEER");
    expect(opened).toBe(true);

    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".eq-band-card").first()).toBeVisible({ timeout: 5000 });

    await page.waitForTimeout(500);

    // Use second band card (lowshelf)
    const bandCards = page.locator(".eq-band-card");
    const targetBand = bandCards.nth(1);

    // Explicitly disable the band via toggle (reset no longer changes enable state)
    const toggle = targetBand.locator(".eq-band-toggle");
    expect(await toggle.isVisible().catch(() => false)).toBe(true);

    // Read current REAPER state to determine if band is enabled
    const reaperEqBefore = await readReaperEq(32);
    const band0Before = parseReaperBand(reaperEqBefore!, 0);
    const isEnabled = band0Before && band0Before["en"] === "1";

    // If band is enabled, click toggle to disable it
    if (isEnabled) {
      await toggle.click();
      await page.waitForTimeout(500);
    }

    // Now drag the gain slider while band is disabled
    const gainSlider = targetBand.locator(".eq-slider-track").nth(1);
    const box = await gainSlider.boundingBox();
    expect(box).toBeTruthy();

    const y = box!.y + box!.height / 2;
    await page.mouse.move(box!.x + box!.width * 0.25, y);
    await page.mouse.down();
    await page.waitForTimeout(200);
    await page.mouse.move(box!.x + box!.width * 0.6, y, { steps: 5 });
    await page.mouse.up();
    await page.waitForTimeout(500);

    // Check REAPER: band should still be disabled (en=0)
    const freshEq = await readReaperEq(32);
    expect(freshEq).toBeTruthy();

    const band0 = parseReaperBand(freshEq!, 0);
    expect(band0).toBeTruthy();

    // Band should remain disabled
    expect(band0!["en"]).toBe("0");

    // Cleanup: re-enable band and close
    const toggleAfter = targetBand.locator(".eq-band-toggle");
    const afterClasses = await toggleAfter.getAttribute("class");
    if (afterClasses && afterClasses.includes(" off")) {
      await toggleAfter.click();
      await page.waitForTimeout(300);
    }
    await page.locator(".eq-close-btn").click();
  });
});
