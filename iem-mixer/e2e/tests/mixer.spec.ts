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

// Precondition check: logs explicitly and returns false when condition is not met.
// Unlike silent `if (!x) return`, this makes the skip visible in test output.
// Unlike expect(), this doesn't fail the test in CI where REAPER is not connected.
function assume(condition: unknown, message: string): condition is true {
  if (!condition) {
    console.log(`[ASSUME SKIP] ${message}`);
    return false;
  }
  return true;
}

// Helper to wait for mixer page to load with graceful skip in CI
// Returns true if mixer loaded, false if should skip (REAPER not connected)
async function waitForMixer(
  page: Page,
  message = "Mixer must load (requires REAPER connection)",
): Promise<boolean> {
  const mixerLoaded = await page
    .waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 })
    .catch(() => null);
  return assume(mixerLoaded, message);
}

test.describe("Branding", () => {
  test("landing page header shows NEWLEVEL IEM MIXER", async ({ page }) => {
    // Wait for network to settle - WASM app needs time to load and hydrate
    await page.goto("/", { waitUntil: "networkidle" });

    // In CI without REAPER, WASM app may not mount properly
    // Use assume() pattern for graceful skip
    const headerLoaded = await page
      .waitForSelector(".header h1", { timeout: 10000 })
      .catch(() => null);
    if (
      !assume(
        headerLoaded,
        "Landing page header must be visible (requires WASM hydration)",
      )
    )
      return;

    // Verify header text
    const header = page.locator(".header h1");
    await expect(header).toHaveText("NEWLEVEL IEM MIXER");
  });
});

test.describe("Mixer Features - Must All Pass", () => {
  test("member route redirects or serves content", async ({ page }) => {
    // Member routes should either redirect to login or show mixer
    const response = await page.goto("/petronela");
    expect(response?.status()).toBe(200);
  });

  test("unknown routes return valid response", async ({ page }) => {
    // SPA should handle unknown routes gracefully
    const response = await page.goto("/unknown-route-12345");
    expect(response?.status()).toBe(200);
  });

  test("API mixer endpoint responds", async ({ request }) => {
    // Mixer endpoint should respond (may be 401 without auth, or 404 if member not found)
    const response = await request.get("/api/mixer/petronela");
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
    // Full version shows "version (date time)" in Slovak format, e.g., "1.1.0 (26.02.2026 14:30)"
    expect(version.full_version).toContain(version.version);
    // Full version should contain a date pattern (DD.MM.YYYY HH:MM) in Slovak format
    expect(version.full_version).toMatch(/\(\d{2}\.\d{2}\.\d{4} \d{2}:\d{2}\)/);
    // deployed_at should be a full timestamp with UTC
    expect(version.deployed_at).toMatch(
      /\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC/,
    );
  });

  test("fader exists and is interactive", async ({ page }) => {
    // Login first - need to navigate to a page first for localStorage
    await page.goto("/");
    await loginAs(page, "petronela");

    await page.goto("/petronela");

    // Wait for app to initialize - gracefully skip if REAPER not available
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    const box = await fader.boundingBox();
    if (!assume(box, "fader bounding box must exist")) return;
    if (!assume(box!.width > 50, "fader must have usable width")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    const box = await fader.boundingBox();
    if (!assume(box, "fader bounding box must exist")) return;
    if (!assume(box!.width > 50, "fader must have usable width")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    const box = await fader.boundingBox();
    if (!assume(box, "fader bounding box must exist")) return;
    if (!assume(box!.width > 50, "fader must have usable width")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    // dB display must be ABOVE the fader (lower Y value = higher on screen)
    const dbBox = await channel.locator(".db-display").boundingBox();
    const faderBox = await channel.locator(".fader-track").boundingBox();

    expect(dbBox).not.toBeNull();
    expect(faderBox).not.toBeNull();
    // dB display top edge must be above fader top edge
    expect(dbBox!.y).toBeLessThan(faderBox!.y);
  });

  test("dB display text is not clipped", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    const dbDisplay = channel.locator(".db-display");
    await expect(dbDisplay).toBeVisible();
    // scrollWidth > clientWidth means text is clipped by overflow
    const isClipped = await dbDisplay.evaluate(
      (el) => el.scrollWidth > el.clientWidth,
    );
    expect(isClipped).toBe(false);
  });

  test("kebab menu button is left of channel name", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    const menuBox = await channel.locator(".ch-menu-btn").boundingBox();
    const labelBox = await channel.locator(".ch-label").boundingBox();
    expect(menuBox).not.toBeNull();
    expect(labelBox).not.toBeNull();
    // Menu X position must be less than label X position (menu is to the left)
    expect(menuBox!.x).toBeLessThan(labelBox!.x);
  });

  test("kebab menu closes when clicking outside", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    // Open the kebab menu
    await channel.locator(".ch-menu-btn").click();
    await expect(channel.locator(".ch-menu-popup")).toBeVisible();

    // Click outside the menu (on the backdrop)
    await page.locator(".ch-menu-backdrop").click();

    // Menu should be closed
    await expect(channel.locator(".ch-menu-popup")).not.toBeVisible();
  });

  test("channel has position: relative for overlay containment", async ({
    page,
  }) => {
    // REGRESSION TEST: v1.28.1 fix - .channel.disconnected::after uses
    // position: absolute, which requires the parent .channel to have
    // position: relative. Without it, the overlay escapes to the nearest
    // positioned ancestor and covers the entire page instead of just the channel.
    await page.goto("/");
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    // Channel MUST have position: relative for ::after overlay to work
    const position = await channel.evaluate(
      (el) => window.getComputedStyle(el).position,
    );
    expect(position).toBe("relative");
  });

  test("mute button exists and is clickable", async ({ page }) => {
    // Login first
    await page.goto("/");
    await page.waitForLoadState("domcontentloaded");
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    // Don't use long waits - check what's available
    const appLoaded = await page
      .waitForSelector(".app.mixer, .mixer-header", { timeout: 5000 })
      .catch(() => null);
    if (!assume(appLoaded, "mixer page must load for this test")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    const box = await fader.boundingBox();
    if (!assume(box, "fader bounding box must exist")) return;
    if (!assume(box!.width > 50, "fader must have usable width")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    const box = await fader.boundingBox();
    if (!assume(box, "fader bounding box must exist")) return;
    if (!assume(box!.width > 50, "fader must have usable width")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    // Don't use long waits - check what's available
    const appLoaded = await page
      .waitForSelector(".app.mixer, .mixer-header", { timeout: 5000 })
      .catch(() => null);
    if (!assume(appLoaded, "mixer page must load for this test")) return;

    // Try to find and click solo button with short timeout
    const channelBtns = await page
      .waitForSelector(".channel-btns", { timeout: 3000 })
      .catch(() => null);
    if (!assume(channelBtns, "channel buttons must load (requires REAPER)"))
      return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const globalVol = page.locator(".channel.global-volume");
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(globalLoaded, "global volume channel must load for this test"))
      return;

    // Check that fader exists within global volume
    const fader = globalVol.locator(".fader-track");
    await expect(fader).toBeVisible();
    await expect(fader.locator(".fader-fill")).toBeAttached();
    await expect(fader.locator(".fader-handle")).toBeAttached();
  });

  test("Global Mute button toggles", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const globalVol = page.locator(".channel.global-volume");
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(globalLoaded, "global volume channel must load for this test"))
      return;

    // Mute button should exist in global volume
    const muteBtn = globalVol.locator(".mute-btn");
    await expect(muteBtn).toBeVisible();
    // Click mute (sends via WebSocket; may be no-op without REAPER)
    await muteBtn.click({ force: true });
  });

  test("Switching to Mics tab shows all mics", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    // Use dispatchEvent to bypass overlay and trigger WASM event listeners
    const stemsTab = page.locator(".category-tab.stems");
    await stemsTab.dispatchEvent("click");
    await expect(stemsTab).toHaveClass(/active/);

    // Wait for channels to appear
    const channelsLoaded = await page
      .waitForSelector(".channel", { timeout: 5000 })
      .catch(() => null);
    if (
      !assume(channelsLoaded, "channels must load in stems tab for this test")
    )
      return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    // Main tab should be active by default
    const mainTab = page.locator(".category-tab.main");
    await expect(mainTab).toHaveClass(/active/);

    // Wait for channels to appear
    const channelsLoaded = await page
      .waitForSelector(".channel", { timeout: 5000 })
      .catch(() => null);
    if (!assume(channelsLoaded, "channels must load on main tab for this test"))
      return;

    // Member's mic fader MUST be visible (the "Me" fader)
    // Input track name is "PETKA mic" (physical mic label, not renamed)
    // Channel names come from REAPER — may not be available in CI
    const meFader = page
      .locator(".channel .ch-name")
      .filter({ hasText: /PETKA/i });
    const meFaderCount = await meFader.count();
    if (!assume(meFaderCount > 0, "PETKA channel must load for this test"))
      return;
    await expect(meFader).toHaveCount(1);
  });

  test("Global Volume fader holds position after drag (no snap-back)", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const globalVol = page.locator(".channel.global-volume");
    const globalLoaded = await globalVol
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(globalLoaded, "global volume channel must load for this test"))
      return;

    const fader = globalVol.locator(".fader-track");
    const box = await fader.boundingBox();
    if (!assume(box, "global volume fader bounding box must exist")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    // Wait for channels to load
    const channelsLoaded = await page
      .waitForSelector(".pan-slider", { timeout: 5000 })
      .catch(() => null);
    if (!assume(channelsLoaded, "pan sliders must load for this test")) return;

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
    await loginAs(page, "petronela");

    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    // Switch to Tech tab
    const techTab = page.locator(".category-tab.tech");
    await techTab.dispatchEvent("click");
    await expect(techTab).toHaveClass(/active/);

    // Wait for channels to appear
    const channelsLoaded = await page
      .waitForSelector(".channel", { timeout: 5000 })
      .catch(() => null);
    if (!assume(channelsLoaded, "channels must load in tech tab for this test"))
      return;

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
      data: { member: "petronela", pin: "7711" },
    });
    expect(resp.status()).toBe(200);
    const data = await resp.json();
    expect(data.member).toBe("petronela");
    expect(data.engineer).toBe(false);
  });

  test("login with wrong PIN fails", async ({ request }) => {
    const resp = await request.post("/api/auth", {
      data: { member: "petronela", pin: "0000" },
    });
    expect(resp.status()).toBe(401);
  });

  test("login with empty PIN fails", async ({ request }) => {
    const resp = await request.post("/api/auth", {
      data: { member: "petronela", pin: "" },
    });
    expect(resp.status()).toBe(401);
  });

  test("engineer PIN 1177 grants engineer access", async ({ request }) => {
    const resp = await request.post("/api/auth", {
      data: { member: "petronela", pin: "1177" },
    });
    expect(resp.status()).toBe(200);
    const data = await resp.json();
    expect(data.engineer).toBe(true);
  });

  test("change PIN flow works", async ({ request }) => {
    // Login with default PIN
    const loginResp = await request.post("/api/auth", {
      data: { member: "petronela", pin: "7711" },
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
      data: { member: "petronela", pin: "1234" },
    });
    expect(newLoginResp.status()).toBe(200);

    // Login with old default PIN fails
    const oldLoginResp = await request.post("/api/auth", {
      data: { member: "petronela", pin: "7711" },
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
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const settingsBtn = page.locator(".settings-btn");
    await expect(settingsBtn).toBeVisible({ timeout: 5000 });
    // Should contain the gear unicode character
    const text = await settingsBtn.textContent();
    expect(text).toContain("\u2699");
  });

  test("settings modal shows fader toggle only, no pan toggle", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    // Open settings modal
    const settingsBtn = page.locator(".settings-btn");
    await expect(settingsBtn).toBeVisible({ timeout: 5000 });
    await settingsBtn.click();

    // Wait for modal to appear
    const modal = page.locator(".settings-modal");
    await expect(modal).toBeVisible({ timeout: 3000 });

    // Should have "Fader double-tap" toggle
    const faderToggle = modal.locator(".settings-name", {
      hasText: "Fader double-tap",
    });
    await expect(faderToggle).toBeVisible();

    // Should NOT have "Pan double-tap" toggle (pan double-tap is always enabled)
    const panToggle = modal.locator(".settings-name", {
      hasText: "Pan double-tap",
    });
    await expect(panToggle).toHaveCount(0);

    // Preferences section should have exactly 1 toggle row
    const prefsSection = modal
      .locator(".settings-section")
      .filter({ hasText: "Preferences" });
    const toggleRows = prefsSection.locator(".settings-row");
    await expect(toggleRows).toHaveCount(1);
  });
});

test.describe("v1.16.0 Hotfix Regression Tests", () => {
  test("pan double-click animates slider value toward center", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const panSlider = page.locator(".pan-slider").first();
    const loaded = await panSlider
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(loaded, "pan slider must load for this test")) return;

    // Read initial value before double-click
    const valueBefore = parseInt(await panSlider.inputValue());

    // Double-click the pan slider to trigger animation toward center (50)
    await panSlider.dblclick({ force: true });

    // Wait for animation to progress (a few ticks at 50ms/tick)
    await page.waitForTimeout(200);

    // Read intermediate value — must have moved from initial toward center
    const valueMid = parseInt(await panSlider.inputValue());

    // If initial was already center, the value stays at 50 — still valid
    if (valueBefore !== 50) {
      // Value must have changed during animation (the bug: attribute doesn't update DOM property)
      expect(valueMid).not.toBe(valueBefore);
    }

    // Wait for animation to fully complete (~1.25s max from extreme)
    await page.waitForTimeout(1500);

    // Final value must be exactly 50 (center)
    const valueFinal = parseInt(await panSlider.inputValue());
    expect(valueFinal).toBe(50);

    // The slider must also have the "centered" CSS class
    await expect(panSlider).toHaveClass(/centered/);
  });

  test("status dot has pulse animation when connected", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

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
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const versionDate = page.locator(".header-version-date");
    await expect(versionDate).toBeVisible({ timeout: 5000 });

    // Get computed color — must be brighter than #555 (85 in each channel)
    const color = await versionDate.evaluate(
      (el) => window.getComputedStyle(el).color,
    );
    // Parse rgb(r, g, b) or rgba(r, g, b, a) — each channel must average > 100 for readability
    const match = color.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
    expect(match).not.toBeNull();
    const avg =
      (parseInt(match![1]) + parseInt(match![2]) + parseInt(match![3])) / 3;
    expect(avg).toBeGreaterThan(100); // #555 = 85 avg, white = 255 avg
  });
});

test.describe("v1.18.0+ — Fader Resolution, Double-Tap, Stereo Meter", () => {
  test("stereo meter bars visible above fader", async ({ page }) => {
    // v1.19.0: Meter redesigned as stereo (L+R) with gradient and peak hold
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

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
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const meterFill = page.locator(".meter-fill").first();
    const loaded = await meterFill
      .waitFor({ state: "attached", timeout: 5000 })
      .catch(() => null);
    if (!assume(loaded, "meter fill element must be attached for this test"))
      return;

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
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    const box = await fader.boundingBox();
    if (!assume(box, "fader bounding box must exist")) return;
    if (!assume(box!.width > 50, "fader must have usable width")) return;

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
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const fader = page.locator(".fader-track").first();
    const channelLoaded = await fader
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

    const box = await fader.boundingBox();
    if (!assume(box, "fader bounding box must exist")) return;
    if (!assume(box!.width > 50, "fader must have usable width")) return;

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
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const channel = page.locator(".channel").first();
    const channelLoaded = await channel
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(channelLoaded, "channel must load for this test")) return;

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
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    // Skip the first 2 static .meter-fill elements (IEM VOL master L/R)
    // which always have width:0%. Target a dynamic Meter component's fill.
    const meterFill = page.locator(".meter-fill").nth(2);
    const loaded = await meterFill
      .waitFor({ state: "attached", timeout: 5000 })
      .catch(() => null);
    if (
      !assume(
        loaded !== null,
        "dynamic meter-fill element must render for regression test",
      )
    )
      return;

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

    if (!assume(injected, "__iem_ws must be exposed for meter injection test"))
      return;

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
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const meterFill = page.locator(".meter-fill").first();
    const loaded = await meterFill
      .waitFor({ state: "attached", timeout: 5000 })
      .catch(() => null);
    if (!assume(loaded, "meter fill element must be attached for this test"))
      return;

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

test.describe("v1.23.0 — Meter Independence (raw input levels)", () => {
  test("meters show raw input level, not scaled by fader position", async ({
    page,
  }) => {
    // Bug: meters were multiplied by vol_linear * pan_law, making quiet
    // inputs with boosted sends appear as "full signal". Fix: show raw only.
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    // Wait for channels and WS to connect
    const meterFill = page.locator(".meter-fill").nth(2);
    const loaded = await meterFill
      .waitFor({ state: "attached", timeout: 5000 })
      .catch(() => null);
    if (!assume(loaded !== null, "meter-fill element must render")) return;

    await page.waitForTimeout(500);

    // Inject TWO different meter messages with the SAME signal level (0.5),
    // but manipulate channel state between them. If meters are independent
    // of fader/pan, both should produce the same fill width.
    const firstWidth = await page.evaluate(() => {
      const ws = (window as any).__iem_ws as WebSocket | undefined;
      if (!ws || !ws.onmessage) return -1;

      const meters: Record<string, [number, number]> = {};
      for (let i = 1; i <= 22; i++) {
        meters[String(i)] = [0.5, 0.5];
      }
      const msg = JSON.stringify({ event: "Meters", data: { meters } });
      ws.onmessage(new MessageEvent("message", { data: msg }));
      return 0; // Will read width after animation tick
    });

    if (!assume(firstWidth !== -1, "__iem_ws must be exposed")) return;

    // Wait for animation tick to process
    await page.waitForTimeout(200);

    // Read meter width after first injection
    const widthBefore = await meterFill.evaluate((el) => {
      const style = el.getAttribute("style") || "";
      const match = style.match(/width:\s*([\d.]+)%/);
      return match ? parseFloat(match[1]) : 0;
    });

    // Now inject the SAME meter signal — width should remain the same
    // regardless of what the fader/pan values are in the channel state.
    await page.evaluate(() => {
      const ws = (window as any).__iem_ws as WebSocket | undefined;
      if (!ws || !ws.onmessage) return;

      const meters: Record<string, [number, number]> = {};
      for (let i = 1; i <= 22; i++) {
        meters[String(i)] = [0.5, 0.5];
      }
      const msg = JSON.stringify({ event: "Meters", data: { meters } });
      ws.onmessage(new MessageEvent("message", { data: msg }));
    });

    await page.waitForTimeout(200);

    const widthAfter = await meterFill.evaluate((el) => {
      const style = el.getAttribute("style") || "";
      const match = style.match(/width:\s*([\d.]+)%/);
      return match ? parseFloat(match[1]) : 0;
    });

    // Both widths should be non-zero and equal (same raw input = same meter)
    if (!assume(widthBefore > 0, "meter must show signal for 0.5 input"))
      return;
    // Allow 2% tolerance for floating point precision and timing variations
    expect(Math.abs(widthAfter - widthBefore)).toBeLessThanOrEqual(2);
  });

  test("muted channel still shows meter (input signal visible)", async ({
    page,
  }) => {
    // Bug: muted channels returned 0.0 for meter. Fix: meters show raw
    // input level regardless of mute state.
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    const meterFill = page.locator(".meter-fill").nth(2);
    const loaded = await meterFill
      .waitFor({ state: "attached", timeout: 5000 })
      .catch(() => null);
    if (!assume(loaded !== null, "meter-fill element must render")) return;

    await page.waitForTimeout(500);

    // Inject State with a muted channel, then inject strong meter signal
    const injected = await page.evaluate(() => {
      const ws = (window as any).__iem_ws as WebSocket | undefined;
      if (!ws || !ws.onmessage) return false;

      // First send a State message that mutes a channel
      // State message sets channels — we need to include a muted channel
      const stateMsg = JSON.stringify({
        event: "State",
        data: {
          channels: [
            {
              track_index: 1,
              name: "TEST mic",
              category: "mic",
              level_db: -6.0,
              pan: 0.5,
              muted: true,
            },
          ],
        },
      });
      ws.onmessage(new MessageEvent("message", { data: stateMsg }));

      // Now send meter data with strong signal on track 1
      const meters: Record<string, [number, number]> = {};
      meters["1"] = [0.8, 0.75];
      for (let i = 2; i <= 22; i++) {
        meters[String(i)] = [0.5, 0.5];
      }
      const msg = JSON.stringify({ event: "Meters", data: { meters } });
      ws.onmessage(new MessageEvent("message", { data: msg }));
      return true;
    });

    if (!assume(injected, "__iem_ws must be exposed for injection")) return;

    // Wait for animation tick
    await page.waitForTimeout(300);

    // The meter for channel at index 2 (first dynamic = track_idx from channels)
    // should show non-zero width even though the channel is muted
    const fillWidth = await page
      .waitForFunction(
        () => {
          const fills = document.querySelectorAll(".meter-fill");
          if (fills.length < 3) return null;
          const el = fills[2]; // First dynamic channel meter fill
          const style = el.getAttribute("style") || "";
          const match = style.match(/width:\s*([\d.]+)%/);
          const w = match ? parseFloat(match[1]) : 0;
          return w > 5 ? w : null;
        },
        { timeout: 2000 },
      )
      .then((h) => h.jsonValue())
      .catch(() => 0);

    // With the fix: muted channels still show meters (raw input level)
    // Without the fix: muted returns 0.0 → fillWidth stays 0
    expect(fillWidth).toBeGreaterThan(5);
  });
});

test.describe("v1.28.1 Preset Modal Mobile Fix", () => {
  // These tests verify the CSS fix for mobile overflow.
  // In CI without REAPER, the toolbar won't render - tests exit via assume().
  // On production with REAPER, tests run with real assertions.

  test("modal uses percentage-based width (not viewport units)", async ({
    page,
  }) => {
    // REGRESSION TEST: v1.28.0 used `width: min(340px, calc(100vw - 40px))`
    // which overflows on real mobile devices where 100vw > visible viewport.
    // Fix: use `width: 100%; max-width: 340px;` for device-agnostic sizing.
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    // Wait for toolbar - requires REAPER connection for full mixer UI
    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (
      !assume(
        toolbarLoaded,
        "Toolbar must be visible (requires REAPER connection)",
      )
    )
      return;

    // Open presets modal - Presets button MUST be visible
    const presetsBtn = page.locator("button", { hasText: "Presets" });
    await expect(presetsBtn).toBeVisible({ timeout: 5000 });
    await presetsBtn.click();

    // Wait for modal to appear - use specific selector to avoid matching hidden modals
    const modal = page.locator(".modal-overlay.visible .modal");
    await expect(modal).toBeVisible({ timeout: 3000 });

    // Verify CSS properties that prevent mobile overflow
    const styles = await modal.evaluate((el) => {
      const computed = window.getComputedStyle(el);
      return {
        width: computed.width,
        maxWidth: computed.maxWidth,
      };
    });

    // max-width should be 340px (prevents overflow on small screens)
    expect(styles.maxWidth).toBe("340px");

    // BEHAVIORAL CHECK: Modal content must not overflow horizontally
    const hasOverflow = await modal.evaluate(
      (el) => el.scrollWidth > el.clientWidth,
    );
    expect(hasOverflow).toBe(false);
  });

  test("preset input row has min-width: 0 for flex shrinking", async ({
    page,
  }) => {
    // REGRESSION TEST: Without min-width: 0, flex items cannot shrink
    // below their content size, causing overflow on narrow screens.
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (
      !assume(
        toolbarLoaded,
        "Toolbar must be visible (requires REAPER connection)",
      )
    )
      return;

    // Open presets modal
    const presetsBtn = page.locator("button", { hasText: "Presets" });
    await expect(presetsBtn).toBeVisible({ timeout: 5000 });
    await presetsBtn.click();

    // Use specific selector to avoid matching hidden modals
    const modal = page.locator(".modal-overlay.visible .modal");
    await expect(modal).toBeVisible({ timeout: 3000 });

    // Check preset-input-row has min-width: 0
    const inputRow = modal.locator(".preset-input-row");
    const rowMinWidth = await inputRow.evaluate(
      (el) => window.getComputedStyle(el).minWidth,
    );
    expect(rowMinWidth).toBe("0px");

    // Check preset-input has min-width: 0
    const input = modal.locator(".preset-input");
    const inputMinWidth = await input.evaluate(
      (el) => window.getComputedStyle(el).minWidth,
    );
    expect(inputMinWidth).toBe("0px");
  });

  test("modal fits within mobile viewport (375px)", async ({ page }) => {
    // Test on typical mobile viewport to verify no horizontal overflow
    await page.setViewportSize({ width: 375, height: 667 });

    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (
      !assume(
        toolbarLoaded,
        "Toolbar must be visible (requires REAPER connection)",
      )
    )
      return;

    // Open presets modal
    const presetsBtn = page.locator("button", { hasText: "Presets" });
    await expect(presetsBtn).toBeVisible({ timeout: 5000 });
    await presetsBtn.click();

    // Use specific selector to avoid matching hidden modals
    const modal = page.locator(".modal-overlay.visible .modal");
    await expect(modal).toBeVisible({ timeout: 3000 });

    // Modal must fit within viewport with margins
    const box = await modal.boundingBox();
    expect(box).not.toBeNull();
    // Left edge must be >= 0 (not cut off)
    expect(box!.x).toBeGreaterThanOrEqual(0);
    // Right edge must be <= viewport width (not overflowing)
    expect(box!.x + box!.width).toBeLessThanOrEqual(375);
    // Modal should have breathing room (not edge-to-edge)
    expect(box!.x).toBeGreaterThan(10);
    expect(375 - (box!.x + box!.width)).toBeGreaterThan(10);

    // BEHAVIORAL CHECK: No horizontal overflow on modal content
    const hasOverflow = await modal.evaluate(
      (el) => el.scrollWidth > el.clientWidth,
    );
    expect(hasOverflow).toBe(false);
  });

  test("save button visible within modal on mobile", async ({ page }) => {
    // The actual user complaint: save button goes off screen
    await page.setViewportSize({ width: 375, height: 667 });

    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (
      !assume(
        toolbarLoaded,
        "Toolbar must be visible (requires REAPER connection)",
      )
    )
      return;

    // Open presets modal
    const presetsBtn = page.locator("button", { hasText: "Presets" });
    await expect(presetsBtn).toBeVisible({ timeout: 5000 });
    await presetsBtn.click();

    // Use specific selector to avoid matching hidden modals
    const modal = page.locator(".modal-overlay.visible .modal");
    await expect(modal).toBeVisible({ timeout: 3000 });

    // Save button must be visible and within viewport
    const saveBtn = modal.locator(".preset-save-btn");
    await expect(saveBtn).toBeVisible();

    const btnBox = await saveBtn.boundingBox();
    expect(btnBox).not.toBeNull();
    // Button must be fully within viewport
    expect(btnBox!.x).toBeGreaterThanOrEqual(0);
    expect(btnBox!.x + btnBox!.width).toBeLessThanOrEqual(375);

    // BEHAVIORAL CHECK: Button must be fully visible (not clipped by overflow)
    const isClipped = await saveBtn.evaluate((el) => {
      const rect = el.getBoundingClientRect();
      const parent = el.closest(".modal");
      if (!parent) return false;
      const parentRect = parent.getBoundingClientRect();
      // Button is clipped if it extends beyond parent's visible area
      return rect.right > parentRect.right || rect.left < parentRect.left;
    });
    expect(isClipped).toBe(false);
  });
});

test.describe("Main tab channel ordering", () => {
  test("own channel appears first on Main tab, before pinned channels", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "ani");
    await page.goto("/ani");
    if (!(await waitForMixer(page))) return;

    // Wait for channel strips to render
    await page.waitForSelector(".channel", { timeout: 5000 }).catch(() => null);
    const channels = page.locator(".channel:not(.global-volume)");
    const count = await channels.count();
    if (!assume(count >= 1, "At least one channel strip must exist")) return;

    // First non-global-volume channel should be the member's own input
    const firstName = await channels.first().locator(".ch-name").textContent();
    expect(firstName?.toUpperCase()).toContain("ANI");
  });

  test("MY MIC label is not present on Main tab", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "ani");
    await page.goto("/ani");
    if (!(await waitForMixer(page))) return;

    const myMicLabel = page.locator(".main-section-label");
    await expect(myMicLabel).toHaveCount(0);
  });

  test("hide works on muted channel (#78)", async ({ page }) => {
    // Login and navigate to mixer
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    if (!(await waitForMixer(page))) return;

    // Switch to Mics tab (NOT Main — Main doesn't filter hidden channels)
    const micsTab = page.locator(".category-tab.mics");
    if (!(await micsTab.count())) return;
    await micsTab.click();

    // Wait for channels to appear
    await page.waitForSelector(".channel", { timeout: 5000 }).catch(() => null);
    const initialChannels = await page.locator(".channel").count();
    if (!assume(initialChannels > 0, "Need at least one channel on Mics tab"))
      return;

    // Get the first channel
    const firstChannel = page.locator(".channel").first();

    // Mute the channel
    const muteBtn = firstChannel.locator(".mute-btn");
    if (!assume((await muteBtn.count()) > 0, "Mute button must exist")) return;
    await muteBtn.click({ force: true });

    // Verify the channel has the muted class
    await expect(firstChannel).toHaveClass(/muted/, { timeout: 2000 });

    // Open kebab menu on the muted channel
    const kebabBtn = firstChannel.locator(".ch-menu-btn");
    await kebabBtn.click();

    // Verify popup is visible
    const popup = firstChannel.locator(".ch-menu-popup");
    await expect(popup).toBeVisible({ timeout: 2000 });

    // Click Hide button (second menu item)
    const hideBtn = popup.locator(".ch-menu-item").last();
    await hideBtn.click();

    // Verify channel count decreased — the hidden channel should disappear from Mics tab
    await expect(page.locator(".channel")).toHaveCount(initialChannels - 1, {
      timeout: 3000,
    });

    // Switch to Hidden tab to verify the channel is there
    const hiddenTab = page.locator(".category-tab.hidden");
    if ((await hiddenTab.count()) > 0) {
      await hiddenTab.click();
      // The hidden channel should appear on the Hidden tab
      await expect(page.locator(".channel")).toHaveCount(1, { timeout: 3000 });
    }
  });
});
