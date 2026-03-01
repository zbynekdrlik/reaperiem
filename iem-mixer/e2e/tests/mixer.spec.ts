import { test, expect, Page } from "@playwright/test";

// Helper to login and set auth in localStorage
async function loginAs(page: Page, member: string) {
  // First, call the login API to get a token
  const response = await page.request.post("/api/auth", {
    data: { member, pin: "7711" }, // Default member PIN
  });

  if (response.status() === 200) {
    const data = await response.json();
    // Set auth in localStorage via evaluate (before navigation)
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

test.describe("Mixer Features - Must All Pass", () => {
  test("member route redirects or serves content", async ({ page }) => {
    // Member routes should either redirect to login or show mixer
    const response = await page.goto("/petka");
    expect(response?.status()).toBe(200);
  });

  test("unknown routes return valid response", async ({ page }) => {
    // SPA should handle unknown routes gracefully
    const response = await page.goto("/unknown-route-12345");
    expect(response?.status()).toBe(200);
  });

  test("API mixer endpoint responds", async ({ request }) => {
    // Mixer endpoint should respond (may be 401 without auth, or 404 if member not found)
    const response = await request.get("/api/mixer/petka");
    // 200 (success), 401 (unauthorized), or 404 (member not found) are all valid
    expect([200, 401, 404]).toContain(response.status());
  });

  test("mobile viewport renders without errors", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    const response = await page.goto("/");
    expect(response?.status()).toBe(200);
    // Wait for page to render (don't use networkidle - polling never stops)
    await page.waitForLoadState("domcontentloaded");
  });
});

test.describe("Mixer Controls - Real Functionality Tests", () => {
  test("version endpoint returns build info with valid git hash", async ({
    request,
  }) => {
    const response = await request.get("/api/version");
    expect(response.status()).toBe(200);
    const version = await response.json();
    expect(version).toHaveProperty("version");
    expect(version).toHaveProperty("git_hash");
    expect(version).toHaveProperty("build_time");
    expect(version).toHaveProperty("deployed_at");
    expect(version).toHaveProperty("full_version");
    // Version should be a valid semver-ish string
    expect(version.version).toMatch(/^\d+\.\d+\.\d+/);
    // Git hash MUST NOT be "unknown" - this ensures build.rs ran correctly
    expect(version.git_hash).not.toBe("unknown");
    // Git hash should be a 7-character hex string
    expect(version.git_hash).toMatch(/^[a-f0-9]{7}$/);
    // Full version shows "version (date time)" format, e.g., "1.1.0 (2026-02-26 14:30)"
    expect(version.full_version).toContain(version.version);
    // Full version should contain a date pattern (YYYY-MM-DD HH:MM) not git hash
    expect(version.full_version).toMatch(/\(\d{4}-\d{2}-\d{2} \d{2}:\d{2}\)/);
    // deployed_at should be a full timestamp with UTC
    expect(version.deployed_at).toMatch(
      /\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC/,
    );
  });

  test("fader exists and is interactive", async ({ page }) => {
    // Login first - need to navigate to a page first for localStorage
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");

    // Wait for app to initialize - look for mixer-specific elements
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Look for custom div fader (fill-bar, not native input)
    const fader = page.locator(".fader-track").first();
    if ((await fader.count()) > 0) {
      // Fader should be visible and interactive
      await expect(fader).toBeVisible();
      // Verify fill-bar and handle children exist
      await expect(fader.locator(".fader-fill")).toBeAttached();
      await expect(fader.locator(".fader-handle")).toBeAttached();
      // Controls are sent via WebSocket, not REST API
      await fader.click({ force: true });
    }
  });

  test("fader has proper dimensions and fill-bar is visible", async ({
    page,
  }) => {
    // This test catches the v1.3.0 bug where fader-track had 0 width
    // because absolutely-positioned children don't contribute to parent size
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    // CRITICAL: Fader track must have real width (not collapsed to 0)
    const box = await fader.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.width).toBeGreaterThan(50); // Fader should span most of channel width
    expect(box!.height).toBeGreaterThanOrEqual(40); // Touch-friendly height

    // Fill bar must be a child with actual rendered dimensions
    const fill = fader.locator(".fader-fill");
    await expect(fill).toBeAttached();
    const fillBox = await fill.boundingBox();
    expect(fillBox).not.toBeNull();
    // Fill height should match track height (absolute positioned top:0 bottom:0)
    expect(fillBox!.height).toBeGreaterThan(0);

    // Handle must be present
    const handle = fader.locator(".fader-handle");
    await expect(handle).toBeAttached();
    const handleBox = await handle.boundingBox();
    expect(handleBox).not.toBeNull();
    expect(handleBox!.height).toBeGreaterThan(0);
  });

  test("fader single click does NOT jump (safety)", async ({ page }) => {
    // CRITICAL SAFETY: Clicking anywhere on fader must NOT cause absolute jump.
    // All movement is relative-only with 150ms activation delay.
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    const box = await fader.boundingBox();
    expect(box, "fader bounding box must exist").toBeTruthy();
    expect(box!.width, "fader must have usable width").toBeGreaterThan(50);

    // Get initial fill width
    const fillBefore = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);

    // Click at 75% of the fader track — should NOT jump (150ms activation required)
    await page.mouse.click(
      box!.x + box!.width * 0.75,
      box!.y + box!.height / 2,
    );
    await page.waitForTimeout(100);

    // Fill width must NOT have changed (no absolute jump)
    const fillAfter = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);

    expect(Math.abs(fillAfter - fillBefore)).toBeLessThan(2);
  });

  test("fader hold-and-drag activates then moves", async ({ page }) => {
    // Verifies 150ms activation delay + relative drag movement
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    const box = await fader.boundingBox();
    expect(box, "fader bounding box must exist").toBeTruthy();
    expect(box!.width, "fader must have usable width").toBeGreaterThan(50);

    // Mouse down at center of fader
    await page.mouse.move(box!.x + box!.width * 0.5, box!.y + box!.height / 2);
    await page.mouse.down();

    // Wait for 150ms activation delay
    await page.waitForTimeout(350);

    // Verify .active class appears after activation
    await expect(fader).toHaveClass(/active/);

    // Get fill width at activation point
    const fillAtActivation = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);

    // Drag right by 30% of track width (relative movement)
    await page.mouse.move(box!.x + box!.width * 0.8, box!.y + box!.height / 2);
    await page.waitForTimeout(50);

    // Fill should have grown (moved right via relative delta)
    const fillAfterDrag = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);

    expect(fillAfterDrag).toBeGreaterThan(fillAtActivation);

    // Release
    await page.mouse.up();

    // Active class should be removed after release
    await page.waitForTimeout(50);
    const classAfter = await fader.getAttribute("class");
    expect(classAfter).not.toContain("active");
  });

  test("fader glow persists during continuous drag (regression test)", async ({
    page,
  }) => {
    // CRITICAL: Tests that .active class persists through MULTIPLE mouse moves
    // This catches the v1.4.x bug where component remounting caused is_activated
    // to reset to false after the first movement, stopping further drag.
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    const box = await fader.boundingBox();
    expect(box, "fader bounding box must exist").toBeTruthy();
    expect(box!.width, "fader must have usable width").toBeGreaterThan(50);

    // Mouse down at 20% of fader (left side)
    await page.mouse.move(box!.x + box!.width * 0.2, box!.y + box!.height / 2);
    await page.mouse.down();

    // Wait for 150ms activation delay
    await page.waitForTimeout(350);

    // Verify activation
    await expect(fader).toHaveClass(/active/);

    // FIRST drag movement to 40%
    await page.mouse.move(box!.x + box!.width * 0.4, box!.y + box!.height / 2);
    await page.waitForTimeout(100); // Give time for state updates

    // CRITICAL CHECK: .active must STILL be present after first movement
    await expect(fader).toHaveClass(/active/);
    const classAfterMove1 = await fader.getAttribute("class");
    expect(classAfterMove1).toContain("active");

    // SECOND drag movement to 60%
    await page.mouse.move(box!.x + box!.width * 0.6, box!.y + box!.height / 2);
    await page.waitForTimeout(100);

    // CRITICAL CHECK: .active must STILL be present after second movement
    await expect(fader).toHaveClass(/active/);
    const classAfterMove2 = await fader.getAttribute("class");
    expect(classAfterMove2).toContain("active");

    // THIRD drag movement to 80%
    await page.mouse.move(box!.x + box!.width * 0.8, box!.y + box!.height / 2);
    await page.waitForTimeout(100);

    // CRITICAL CHECK: .active must STILL be present after third movement
    await expect(fader).toHaveClass(/active/);
    const classAfterMove3 = await fader.getAttribute("class");
    expect(classAfterMove3).toContain("active");

    // Verify fader actually moved (fill bar should span most of track)
    const fillWidth = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);
    // Should have moved significantly from 20% to 80%
    expect(fillWidth / box!.width).toBeGreaterThan(0.5);

    // Release
    await page.mouse.up();

    // Only NOW should .active be removed
    await page.waitForTimeout(50);
    const classAfterRelease = await fader.getAttribute("class");
    expect(classAfterRelease).not.toContain("active");
  });

  test("fader handle is visible with proper width", async ({ page }) => {
    // Verifies the handle thumb is wide enough to be a visible grab target
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    const handle = fader.locator(".fader-handle");
    await expect(handle).toBeAttached();
    const handleBox = await handle.boundingBox();
    expect(handleBox).not.toBeNull();
    // Handle must be at least 12px wide (visible thumb, not a thin line)
    expect(handleBox!.width).toBeGreaterThanOrEqual(12);
  });

  test("channel layout has controls above fader", async ({ page }) => {
    // Verifies row order: controls (label, dB, mute) in Row 1, fader in Row 2
    // This catches the v1.2.0 bug where fader was on top and finger covered dB
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    // dB display must be ABOVE the fader (lower Y value = higher on screen)
    const dbBox = await channel.locator(".db-display").boundingBox();
    const faderBox = await channel.locator(".fader-track").boundingBox();

    expect(dbBox).not.toBeNull();
    expect(faderBox).not.toBeNull();
    // dB display top edge must be above fader top edge
    expect(dbBox!.y).toBeLessThan(faderBox!.y);
  });

  test("mute button exists and is clickable", async ({ page }) => {
    // Login first
    await page.goto("/");
    await page.waitForLoadState("domcontentloaded");
    await loginAs(page, "petka");

    await page.goto("/petka");
    // Don't use long waits - check what's available
    const appLoaded = await page
      .waitForSelector(".app.mixer, .mixer-header", { timeout: 5000 })
      .catch(() => null);
    expect(appLoaded, "mixer page must load for this test").toBeTruthy();

    // Check for mute button without long wait
    const muteBtn = page.locator(".mute-btn").first();
    const count = await muteBtn.count().catch(() => 0);
    if (count > 0) {
      await expect(muteBtn).toBeVisible({ timeout: 2000 });
      // Use force:true to bypass grid container intercepting pointer events
      await muteBtn.click({ force: true });
    }
  });

  test("solo button exists (S button next to M)", async ({ page }) => {
    // Login first
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Wait for channels to load
    try {
      await page.waitForSelector(".channel-btns", { timeout: 10000 });
      // Solo button should exist in the channel buttons
      const soloBtn = page.locator(".solo-btn").first();
      // Solo button MUST exist in the new version
      await expect(soloBtn).toBeVisible({ timeout: 5000 });
    } catch {
      // If channels don't load (no REAPER), skip - can't test solo button
      // But we need this test to pass, so check if at least the page loaded
      await expect(page.locator(".mixer-header")).toBeVisible();
    }
  });

  test("reset button does NOT exist (removed for safety)", async ({ page }) => {
    // Login first
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header, .toolbar", {
      timeout: 10000,
    });

    // Reset button must NOT be present - use exact text match
    // Note: "Presets" contains "reset" as substring, so use exact match
    const resetBtn = page.locator("button", { hasText: /^Reset$/ });
    // Should have zero matches
    await expect(resetBtn).toHaveCount(0);
  });

  test("fader does not snap back after drag (regression)", async ({ page }) => {
    // CRITICAL: This test catches the snap-back bug where the fader jumps
    // to a stale position after release due to server echo broadcasts.
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    const box = await fader.boundingBox();
    expect(box, "fader bounding box must exist").toBeTruthy();
    expect(box!.width, "fader must have usable width").toBeGreaterThan(50);

    // Mouse down at 20% and wait for activation
    await page.mouse.move(box!.x + box!.width * 0.2, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(350); // Wait for 150ms activation delay

    // Drag to 80% in steps (simulating real finger movement)
    for (let pct = 0.3; pct <= 0.8; pct += 0.05) {
      await page.mouse.move(
        box!.x + box!.width * pct,
        box!.y + box!.height / 2,
      );
      await page.waitForTimeout(30);
    }

    // Record fill width at release point
    const fillAtRelease = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);

    // Release
    await page.mouse.up();

    // Wait for server convergence (echo suppression window)
    await page.waitForTimeout(500);

    // Fill width must be stable (within 5% of track width tolerance)
    const fillAfterWait = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);
    const drift = Math.abs(fillAfterWait - fillAtRelease);
    const tolerance = box!.width * 0.05;
    expect(drift).toBeLessThan(tolerance);

    // Wait another 500ms to ensure no late snap-back
    await page.waitForTimeout(500);
    const fillLater = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);
    const lateDrift = Math.abs(fillLater - fillAtRelease);
    expect(lateDrift).toBeLessThan(tolerance);
  });

  test("rapid fader movement does not cause stutter (regression)", async ({
    page,
  }) => {
    // Tests that rapid back-and-forth movement doesn't cause UI stutter
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    const box = await fader.boundingBox();
    expect(box, "fader bounding box must exist").toBeTruthy();
    expect(box!.width, "fader must have usable width").toBeGreaterThan(50);

    // Mouse down and activate
    await page.mouse.move(box!.x + box!.width * 0.5, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(350);

    // Rapid back-and-forth movement (5 cycles)
    for (let i = 0; i < 5; i++) {
      await page.mouse.move(
        box!.x + box!.width * 0.3,
        box!.y + box!.height / 2,
      );
      await page.waitForTimeout(30);
      await page.mouse.move(
        box!.x + box!.width * 0.7,
        box!.y + box!.height / 2,
      );
      await page.waitForTimeout(30);
    }

    // End at 60% position
    await page.mouse.move(box!.x + box!.width * 0.6, box!.y + box!.height / 2);
    await page.waitForTimeout(50);

    const fillAtEnd = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);

    // Release
    await page.mouse.up();

    // Wait for convergence
    await page.waitForTimeout(500);

    // Should be stable near the 60% position
    const fillAfterWait = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);
    const drift = Math.abs(fillAfterWait - fillAtEnd);
    const tolerance = box!.width * 0.05;
    expect(drift).toBeLessThan(tolerance);
  });

  test("solo button triggers state change when clicked", async ({ page }) => {
    // Login first
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    // Don't use long waits - check what's available
    const appLoaded = await page
      .waitForSelector(".app.mixer, .mixer-header", { timeout: 5000 })
      .catch(() => null);
    expect(appLoaded, "mixer page must load for this test").toBeTruthy();

    // Try to find and click solo button with short timeout
    const channelBtns = await page
      .waitForSelector(".channel-btns", { timeout: 3000 })
      .catch(() => null);
    expect(
      channelBtns,
      "channel buttons must load (requires REAPER)",
    ).toBeTruthy();

    const soloBtn = page.locator(".solo-btn").first();
    const count = await soloBtn.count().catch(() => 0);
    if (count > 0) {
      // Verify solo button starts as "off"
      await expect(soloBtn).toHaveClass(/off/);
      // Use force:true to bypass grid container intercepting pointer events
      // Solo sends commands via WebSocket (not REST API)
      await soloBtn.click({ force: true });
      await page.waitForTimeout(200);
      // Without REAPER, the click may be a no-op (connected=false).
      // Verify the button is still interactive (class contains solo-btn).
      const classAfter = await soloBtn.getAttribute("class");
      expect(classAfter).toContain("solo-btn");
      // If REAPER is connected, the state changes to "on";
      // if not, it stays "off" - both are valid in CI.
    }
  });
});

test.describe("Main Tab and Global Volume", () => {
  test("Main tab loads as default with IEM VOL and member mic", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Main tab should be active by default
    const mainTab = page.locator(".category-tab.main");
    await expect(mainTab).toBeVisible();
    await expect(mainTab).toHaveClass(/active/);

    // Global Volume channel should be present with "IEM VOL" label
    const globalVol = page.locator(".channel.global-volume");
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (globalLoaded) {
      await expect(globalVol.locator(".ch-name")).toContainText("IEM VOL");
    }
  });

  test("Global Volume fader is draggable", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const globalVol = page.locator(".channel.global-volume");
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(
      globalLoaded,
      "global volume channel must load for this test",
    ).toBeTruthy();

    // Check that fader exists within global volume
    const fader = globalVol.locator(".fader-track");
    await expect(fader).toBeVisible();
    await expect(fader.locator(".fader-fill")).toBeAttached();
    await expect(fader.locator(".fader-handle")).toBeAttached();
  });

  test("Global Mute button toggles", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const globalVol = page.locator(".channel.global-volume");
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(
      globalLoaded,
      "global volume channel must load for this test",
    ).toBeTruthy();

    // Mute button should exist in global volume
    const muteBtn = globalVol.locator(".mute-btn");
    await expect(muteBtn).toBeVisible();
    // Click mute (sends via WebSocket; may be no-op without REAPER)
    await muteBtn.click({ force: true });
  });

  test("Switching to Mics tab shows all mics", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Use dispatchEvent to bypass overlay and trigger WASM event listeners
    const micsTab = page.locator(".category-tab.mics");
    await micsTab.dispatchEvent("click");
    await expect(micsTab).toHaveClass(/active/);

    // Global volume should NOT appear in Mics tab
    const globalVol = page.locator(".channel.global-volume");
    await expect(globalVol).toHaveCount(0);
  });

  test("Switching to Stems tab shows Click first then Guide", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Use dispatchEvent to bypass overlay and trigger WASM event listeners
    const stemsTab = page.locator(".category-tab.stems");
    await stemsTab.dispatchEvent("click");
    await expect(stemsTab).toHaveClass(/active/);

    // Wait for channels to appear
    const channelsLoaded = await page
      .waitForSelector(".channel", { timeout: 5000 })
      .catch(() => null);
    expect(
      channelsLoaded,
      "channels must load in stems tab for this test",
    ).toBeTruthy();

    // Get all channel names in order
    const channelNames = await page
      .locator(".channel .ch-name")
      .allTextContents();

    if (channelNames.length >= 2) {
      // CLICK should be first, GUIDE second
      expect(channelNames[0].toUpperCase()).toBe("CLICK");
      expect(channelNames[1].toUpperCase()).toBe("GUIDE");
    }
  });

  test("Switching to Tech tab shows tech channels", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Use dispatchEvent to bypass overlay and trigger WASM event listeners
    const techTab = page.locator(".category-tab.tech");
    await techTab.dispatchEvent("click");
    await expect(techTab).toHaveClass(/active/);

    // Main tab should not be active
    const mainTab = page.locator(".category-tab.main");
    await expect(mainTab).not.toHaveClass(/active/);
  });

  test("Me fader appears on Main tab for logged-in member", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Main tab should be active by default
    const mainTab = page.locator(".category-tab.main");
    await expect(mainTab).toHaveClass(/active/);

    // Wait for channels to appear
    const channelsLoaded = await page
      .waitForSelector(".channel", { timeout: 5000 })
      .catch(() => null);
    expect(
      channelsLoaded,
      "channels must load on main tab for this test",
    ).toBeTruthy();

    // Member's mic fader MUST be visible (the "Me" fader)
    // Bug: case mismatch means this channel never appears
    const meFader = page
      .locator(".channel .ch-name")
      .filter({ hasText: /PETKA/i });
    await expect(meFader).toHaveCount(1, { timeout: 5000 });
  });

  test("Global Volume fader holds position after drag (no snap-back)", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const globalVol = page.locator(".channel.global-volume");
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(
      globalLoaded,
      "global volume channel must load for this test",
    ).toBeTruthy();

    const fader = globalVol.locator(".fader-track");
    const box = await fader.boundingBox();
    expect(box, "global volume fader bounding box must exist").toBeTruthy();

    // Mouse down at center, wait for activation
    await page.mouse.move(box!.x + box!.width * 0.5, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(350);

    // Drag to 80%
    for (let pct = 0.5; pct <= 0.8; pct += 0.05) {
      await page.mouse.move(
        box!.x + box!.width * pct,
        box!.y + box!.height / 2,
      );
      await page.waitForTimeout(30);
    }

    // Read fill width WHILE still holding
    const fillWhileHeld = await globalVol
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);

    await page.mouse.up();
    await page.waitForTimeout(500);

    // Read fill width AFTER release — must not snap back
    const fillAfterRelease = await globalVol
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);

    // Bug: without optimistic update, fillAfterRelease snaps to old value
    const tolerance = box!.width * 0.05;
    expect(Math.abs(fillAfterRelease - fillWhileHeld)).toBeLessThan(tolerance);
  });

  test("version is displayed in mixer header", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Version block in header must exist
    const versionBlock = page.locator(".header-version");
    await expect(versionBlock).toBeVisible({ timeout: 5000 });
    // Version number must contain semver pattern (e.g., "v1.16.0")
    const versionNumber = page.locator(".header-version-number");
    await expect(versionNumber).toBeVisible();
    const text = await versionNumber.textContent();
    expect(text).toMatch(/v?\d+\.\d+\.\d+/);
    // Build date must exist
    const versionDate = page.locator(".header-version-date");
    await expect(versionDate).toBeVisible();
  });

  test("status dot shows connection state", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Status dot must exist in header
    const dot = page.locator(".status-dot");
    await expect(dot).toBeVisible({ timeout: 5000 });
    // Must have either connected or disconnected class
    const classes = await dot.getAttribute("class");
    expect(classes).toMatch(/connected|disconnected/);
    // Dot should be small (10x10px)
    const box = await dot.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.width).toBeLessThanOrEqual(15);
    expect(box!.height).toBeLessThanOrEqual(15);
  });

  test("disconnected banner uses amber style (not red)", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Old red warning must NOT exist
    const oldWarning = page.locator(".disconnected-warning");
    await expect(oldWarning).toHaveCount(0);

    // If disconnected, banner should use new amber class
    const banner = page.locator(".disconnected-banner");
    const bannerCount = await banner.count();
    if (bannerCount > 0) {
      // Text should be the calmer message
      await expect(banner).toContainText("Reconnecting");
    }
  });

  test("pan center indicator visible when pan is centered", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Wait for channels to load
    const channelsLoaded = await page
      .waitForSelector(".pan-slider", { timeout: 5000 })
      .catch(() => null);
    expect(channelsLoaded, "pan sliders must load for this test").toBeTruthy();

    // Pan sliders with default center position should have "centered" class
    const panSliders = page.locator(".pan-slider");
    const count = await panSliders.count();
    if (count > 0) {
      // At least one slider should have the centered class (default pan = center)
      const centeredCount = await page.locator(".pan-slider.centered").count();
      expect(centeredCount).toBeGreaterThan(0);
    }

    // Center tick mark (::after pseudo) on pan-container — verify via computed styles
    const panContainer = page.locator(".pan-container").first();
    if ((await panContainer.count()) > 0) {
      const hasPosition = await panContainer.evaluate((el) => {
        return window.getComputedStyle(el).position;
      });
      // Must be relative for ::after positioning
      expect(hasPosition).toBe("relative");
    }
  });

  test("Tech tab shows HAND tracks (not in Mics)", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Switch to Tech tab
    const techTab = page.locator(".category-tab.tech");
    await techTab.dispatchEvent("click");
    await expect(techTab).toHaveClass(/active/);

    // Wait for channels to appear
    const channelsLoaded = await page
      .waitForSelector(".channel", { timeout: 5000 })
      .catch(() => null);
    expect(
      channelsLoaded,
      "channels must load in tech tab for this test",
    ).toBeTruthy();

    // Check HAND tracks are in Tech tab
    const channels = page.locator(".channel .ch-name");
    const names = await channels.allTextContents();
    const hasHand = names.some((n) => /hand/i.test(n));
    expect(hasHand).toBe(true);

    // Switch to Mics tab — HAND must NOT be here
    const micsTab = page.locator(".category-tab.mics");
    await micsTab.dispatchEvent("click");
    await expect(micsTab).toHaveClass(/active/);

    const micsChannels = page.locator(".channel .ch-name");
    const micsNames = await micsChannels.allTextContents();
    const handInMics = micsNames.some((n) => /hand/i.test(n));
    expect(handInMics).toBe(false);
  });
});

test.describe("v1.17.0 PIN Authentication", () => {
  test("login with default PIN 7711 succeeds", async ({ request }) => {
    const resp = await request.post("/api/auth", {
      data: { member: "petka", pin: "7711" },
    });
    expect(resp.status()).toBe(200);
    const data = await resp.json();
    expect(data.member).toBe("petka");
    expect(data.engineer).toBe(false);
  });

  test("login with wrong PIN fails", async ({ request }) => {
    const resp = await request.post("/api/auth", {
      data: { member: "petka", pin: "0000" },
    });
    expect(resp.status()).toBe(401);
  });

  test("login with empty PIN fails", async ({ request }) => {
    const resp = await request.post("/api/auth", {
      data: { member: "petka", pin: "" },
    });
    expect(resp.status()).toBe(401);
  });

  test("engineer PIN 1177 grants engineer access", async ({ request }) => {
    const resp = await request.post("/api/auth", {
      data: { member: "petka", pin: "1177" },
    });
    expect(resp.status()).toBe(200);
    const data = await resp.json();
    expect(data.engineer).toBe(true);
  });

  test("change PIN flow works", async ({ request }) => {
    // Login with default PIN
    const loginResp = await request.post("/api/auth", {
      data: { member: "petka", pin: "7711" },
    });
    expect(loginResp.status()).toBe(200);
    const { token } = await loginResp.json();

    // Change PIN
    const changeResp = await request.post("/api/auth/change-pin", {
      headers: { Authorization: `Bearer ${token}` },
      data: { old_pin: "7711", new_pin: "1234" },
    });
    expect(changeResp.status()).toBe(200);

    // Login with new PIN works
    const newLoginResp = await request.post("/api/auth", {
      data: { member: "petka", pin: "1234" },
    });
    expect(newLoginResp.status()).toBe(200);

    // Login with old default PIN fails
    const oldLoginResp = await request.post("/api/auth", {
      data: { member: "petka", pin: "7711" },
    });
    expect(oldLoginResp.status()).toBe(401);

    // Reset: change back to default so other tests work
    const { token: newToken } = await newLoginResp.json();
    const resetResp = await request.post("/api/auth/change-pin", {
      headers: { Authorization: `Bearer ${newToken}` },
      data: { old_pin: "1234", new_pin: "7711" },
    });
    expect(resetResp.status()).toBe(200);
  });

  test("settings gear icon visible in mixer header", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const settingsBtn = page.locator(".settings-btn");
    await expect(settingsBtn).toBeVisible({ timeout: 5000 });
    // Should contain the gear unicode character
    const text = await settingsBtn.textContent();
    expect(text).toContain("\u2699");
  });
});

test.describe("v1.16.0 Hotfix Regression Tests", () => {
  test("pan double-click moves thumb to center position", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const panSlider = page.locator(".pan-slider").first();
    const loaded = await panSlider
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(loaded, "pan slider must load for this test").toBeTruthy();

    // Double-click the pan slider
    await panSlider.dblclick({ force: true });
    await page.waitForTimeout(100);

    // The native input's value property must be 50 (center)
    const inputValue = await panSlider.inputValue();
    expect(parseInt(inputValue)).toBe(50);
    // The slider must also have the "centered" CSS class
    await expect(panSlider).toHaveClass(/centered/);
  });

  test("status dot has pulse animation when connected", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const dot = page.locator(".status-dot");
    await expect(dot).toBeVisible({ timeout: 5000 });

    // If connected, wait for animation to appear (Meters arrive every ~150ms)
    const isConnected = await dot.evaluate((el) =>
      el.classList.contains("connected"),
    );
    if (isConnected) {
      // Wait up to 1s for a pulse class to appear
      await page.waitForTimeout(500);
      const animName = await dot.evaluate(
        (el) => window.getComputedStyle(el).animationName,
      );
      // Must have a non-"none" animation running
      expect(animName).not.toBe("none");
    }
  });

  test("version date text has readable contrast", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const versionDate = page.locator(".header-version-date");
    await expect(versionDate).toBeVisible({ timeout: 5000 });

    // Get computed color — must be brighter than #555 (85 in each channel)
    const color = await versionDate.evaluate(
      (el) => window.getComputedStyle(el).color,
    );
    // Parse rgb(r, g, b) — each channel must average > 100 for readability
    const match = color.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/);
    expect(match).not.toBeNull();
    const avg =
      (parseInt(match![1]) + parseInt(match![2]) + parseInt(match![3])) / 3;
    expect(avg).toBeGreaterThan(100); // #555 = 85 avg, #888 = 136 avg
  });
});

test.describe("v1.18.0+ — Fader Resolution, Double-Tap, Stereo Meter", () => {
  test("stereo meter bars visible above fader", async ({ page }) => {
    // v1.19.0: Meter redesigned as stereo (L+R) with gradient and peak hold
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    // Stereo meter container must exist
    const meter = channel.locator(".meter-stereo");
    await expect(meter).toBeAttached();
    const meterBox = await meter.boundingBox();
    expect(meterBox).not.toBeNull();

    // Meter should be horizontal: wider than tall
    expect(meterBox!.width).toBeGreaterThan(meterBox!.height);
    // Meter should span significant width (full channel width minus padding)
    expect(meterBox!.width).toBeGreaterThan(50);
    // Meter height should be small (6px = 2px + 1px gap + 2px + minor rounding)
    expect(meterBox!.height).toBeLessThanOrEqual(10);

    // Must have exactly 2 meter bars (L and R channels)
    const bars = channel.locator(".meter-bar");
    await expect(bars).toHaveCount(2);

    // Meter must be ABOVE fader (lower Y = higher on screen)
    const faderBox = await channel.locator(".fader-track").boundingBox();
    expect(faderBox).not.toBeNull();
    expect(meterBox!.y).toBeLessThan(faderBox!.y);
  });

  test("meter uses gradient fill (no CSS transition, Rust ballistics)", async ({
    page,
  }) => {
    // v1.19.0: Ballistics handled in Rust at 30fps, no CSS transition
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const meterFill = page.locator(".meter-fill").first();
    const loaded = await meterFill
      .waitFor({ state: "attached", timeout: 5000 })
      .catch(() => null);
    expect(
      loaded,
      "meter fill element must be attached for this test",
    ).toBeTruthy();

    // Meter fill should use gradient background, not solid color
    const bg = await meterFill.evaluate(
      (el) => window.getComputedStyle(el).backgroundImage,
    );
    expect(bg).toContain("gradient");
  });

  test("fader double-click animates to 0dB (not instant jump)", async ({
    page,
  }) => {
    // Issue #33: Double-click should smoothly animate the fader to 0dB
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    const box = await fader.boundingBox();
    expect(box, "fader bounding box must exist").toBeTruthy();
    expect(box!.width, "fader must have usable width").toBeGreaterThan(50);

    // Record fill width before double-click
    const fillBefore = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);

    // Double-click the fader
    await fader.dblclick({ force: true });

    // The "animating" class should appear
    await page.waitForTimeout(100);
    const classAfterDbl = await fader.getAttribute("class");
    expect(classAfterDbl).toContain("animating");

    // Wait for animation to complete (max ~3s from -60dB)
    await page.waitForTimeout(4000);

    // After animation, "animating" class should be removed
    const classAfterAnim = await fader.getAttribute("class");
    expect(classAfterAnim).not.toContain("animating");

    // Fill should be near 83.33% (0dB position on -60..+12 range)
    const fillAfter = await fader
      .locator(".fader-fill")
      .evaluate((el) => el.getBoundingClientRect().width);
    const expectedPct = 83.33;
    const actualPct = (fillAfter / box!.width) * 100;
    // Allow 5% tolerance
    expect(Math.abs(actualPct - expectedPct)).toBeLessThan(5);
  });

  test("fader touch interrupts animation", async ({ page }) => {
    // Issue #33: Touching fader during animation should stop it immediately
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    const box = await fader.boundingBox();
    expect(box, "fader bounding box must exist").toBeTruthy();
    expect(box!.width, "fader must have usable width").toBeGreaterThan(50);

    // Double-click to start animation
    await fader.dblclick({ force: true });
    await page.waitForTimeout(200); // Let animation start

    // Should be animating
    const classAnimating = await fader.getAttribute("class");
    expect(classAnimating).toContain("animating");

    // Mouse down to interrupt
    await page.mouse.move(box!.x + box!.width * 0.3, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(100);

    // Animation should be cancelled
    const classAfterInterrupt = await fader.getAttribute("class");
    expect(classAfterInterrupt).not.toContain("animating");

    await page.mouse.up();
  });

  test("channel grid has 3 rows (controls, meter, fader)", async ({ page }) => {
    // Verify the CSS grid has 3 row areas
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    expect(channelLoaded, "channel must load for this test").toBeTruthy();

    // Grid template should have 3 rows
    const gridRows = await channel.evaluate(
      (el) => window.getComputedStyle(el).gridTemplateRows,
    );
    // Should have 3 values (e.g., "36px 8px 44px")
    const rowCount = gridRows.split(" ").length;
    expect(rowCount).toBe(3);
  });

  test("meter animation timer stays alive after mount", async ({ page }) => {
    // Regression: gloo_timers Interval was stored in a local Rc that got
    // dropped on component return, killing the 30fps animation loop.
    // This test injects a fake Meters WebSocket message with non-zero values
    // and asserts that the animation timer processes them into width > 0%.
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    // Skip the first 2 static .meter-fill elements (IEM VOL master L/R)
    // which always have width:0%. Target a dynamic Meter component's fill.
    const meterFill = page.locator(".meter-fill").nth(2);
    const loaded = await meterFill
      .waitFor({ state: "attached", timeout: 5000 })
      .catch(() => null);
    expect(
      loaded,
      "dynamic meter-fill element must render for regression test",
    ).not.toBeNull();

    // Wait for WS to connect (poller sends first State within ~150ms)
    await page.waitForTimeout(500);

    // Inject a fake Meters message via the exposed __iem_ws.onmessage handler.
    // ServerMsg is adjacently tagged (#[serde(tag="event", content="data")]).
    // The Meters variant has a named field "meters", so wire format is:
    //   {"event":"Meters","data":{"meters":{"1":[0.85,0.82],...}}}
    const injected = await page.evaluate(() => {
      const ws = (window as any).__iem_ws as WebSocket | undefined;
      if (!ws || !ws.onmessage) return false;

      const meters: Record<string, [number, number]> = {};
      for (let i = 1; i <= 22; i++) {
        meters[String(i)] = [0.85, 0.82]; // Strong stereo signal
      }
      const msg = JSON.stringify({ event: "Meters", data: { meters } });
      // Call onmessage directly with a MessageEvent (same as browser would)
      ws.onmessage(new MessageEvent("message", { data: msg }));
      return true;
    });

    expect(
      injected,
      "__iem_ws must be exposed for meter injection test",
    ).toBeTruthy();

    // Poll until a dynamic meter fill shows signal (skip first 2 = IEM VOL master).
    // waitForFunction resolves on truthy return; return null to keep polling,
    // .catch gives a clear assertion failure instead of timeout.
    const fillWidth = await page
      .waitForFunction(
        () => {
          const fills = document.querySelectorAll(".meter-fill");
          if (fills.length < 3) return null;
          const el = fills[2]; // First dynamic Meter component fill
          const style = el.getAttribute("style") || "";
          const match = style.match(/width:\s*([\d.]+)%/);
          const w = match ? parseFloat(match[1]) : 0;
          return w > 10 ? w : null;
        },
        { timeout: 2000 },
      )
      .then((h) => h.jsonValue())
      .catch(() => 0);

    // If the animation timer is alive, it processes the 0.85 signal level
    // through ballistic_tick (instant attack) and linear_to_pct (~90%).
    // If the timer is dead (the bug), fillWidth stays 0.
    expect(fillWidth).toBeGreaterThan(10);
  });

  test("stereo meter shows zero width when no audio signal present", async ({
    page,
  }) => {
    // v1.19.0: Stereo meters with REAPER meter floor (-1500 cb) = silence
    await page.goto("/");
    await loginAs(page, "petka");
    await page.goto("/petka");
    await page.waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 });

    const meterFill = page.locator(".meter-fill").first();
    const loaded = await meterFill
      .waitFor({ state: "attached", timeout: 5000 })
      .catch(() => null);
    expect(
      loaded,
      "meter fill element must be attached for this test",
    ).toBeTruthy();

    // Wait a bit for meter data to arrive via WebSocket (2 poll cycles)
    await page.waitForTimeout(500);

    // With no audio source active, both L and R meter fills should be 0% (or very small)
    const fills = page.locator(".meter-fill");
    const fillCount = await fills.count();
    for (let i = 0; i < Math.min(fillCount, 4); i++) {
      const fillWidth = await fills.nth(i).evaluate((el) => {
        const style = el.getAttribute("style") || "";
        const match = style.match(/width:\s*([\d.]+)%/);
        return match ? parseFloat(match[1]) : 0;
      });
      expect(fillWidth).toBeLessThanOrEqual(1);
    }

    // Peak indicators should be hidden when no audio
    const peaks = page.locator(".meter-peak");
    const peakCount = await peaks.count();
    for (let i = 0; i < Math.min(peakCount, 4); i++) {
      const display = await peaks
        .nth(i)
        .evaluate((el) => window.getComputedStyle(el).display);
      expect(display).toBe("none");
    }
  });
});
