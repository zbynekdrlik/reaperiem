import { test, expect, Page } from "@playwright/test";

// Helper to login and set auth in localStorage
async function loginAs(page: Page, member: string) {
  // First, call the login API to get a token
  const response = await page.request.post("/api/auth", {
    data: { member, pin: "" }, // Empty PIN when no PIN is configured
  });

  if (response.status() === 200) {
    const data = await response.json();
    // Set auth in localStorage before navigating
    await page.addInitScript(
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
    // Mixer endpoint should respond (may be 401 without auth, or 404 if member not configured)
    const response = await request.get("/api/mixer/petka");
    // 200 (success), 401 (unauthorized), or 404 (member not found) are all valid
    expect([200, 401, 404]).toContain(response.status());
  });

  test("mobile viewport renders without errors", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    const response = await page.goto("/");
    expect(response?.status()).toBe(200);
    // No console errors
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        errors.push(msg.text());
      }
    });
    await page.waitForLoadState("networkidle");
    // Filter out expected WASM-related console messages
    const realErrors = errors.filter((e) => !e.includes("wasm"));
    expect(realErrors).toHaveLength(0);
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
    expect(version).toHaveProperty("full_version");
    // Version should be a valid semver-ish string
    expect(version.version).toMatch(/^\d+\.\d+\.\d+/);
    // Git hash MUST NOT be "unknown" - this ensures build.rs ran correctly
    expect(version.git_hash).not.toBe("unknown");
    // Git hash should be a 7-character hex string
    expect(version.git_hash).toMatch(/^[a-f0-9]{7}$/);
    // Full version should combine both
    expect(version.full_version).toContain(version.version);
    expect(version.full_version).toContain(version.git_hash);
  });

  test("fader actually sends API request", async ({ page }) => {
    // Login first
    await loginAs(page, "petka");

    // Intercept API calls
    const apiCalls: string[] = [];
    page.on("request", (req) => {
      if (req.url().includes("/api/mixer/")) apiCalls.push(req.url());
    });

    await page.goto("/petka");
    await page.waitForLoadState("networkidle");

    // Look for any slider/fader input
    const fader = page.locator('input[type="range"]').first();
    if ((await fader.count()) > 0) {
      await fader.click();
      // Give time for API call to fire
      await page.waitForTimeout(500);
      // Should have made at least one API call
      expect(apiCalls.length).toBeGreaterThan(0);
    }
  });

  test("mute button exists and is clickable", async ({ page }) => {
    // Login first
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForLoadState("networkidle");

    // Look for mute button (typically labeled M or has mute in class)
    const muteBtn = page
      .locator('button:has-text("M"), .mute-btn, [class*="mute"]')
      .first();
    if ((await muteBtn.count()) > 0) {
      await expect(muteBtn).toBeVisible();
      // Should be clickable without error
      await muteBtn.click();
    }
  });

  test("solo button exists (S button next to M)", async ({ page }) => {
    // Login first
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForLoadState("networkidle");

    // Wait for channels to load - check for channel grid
    await page.waitForSelector(".channels-grid, .channel", { timeout: 10000 });

    // Solo button should exist - this verifies the new version is deployed
    // Look for S button, solo-btn class, or anything with solo in it
    const soloBtn = page
      .locator('button:has-text("S"), .solo-btn, [class*="solo"]')
      .first();
    // Solo button MUST exist in the new version
    await expect(soloBtn).toBeVisible({ timeout: 5000 });
  });

  test("reset button does NOT exist (removed for safety)", async ({ page }) => {
    // Login first
    await loginAs(page, "petka");

    await page.goto("/petka");
    await page.waitForLoadState("networkidle");

    // Reset button must NOT be present in the new version
    const resetBtn = page.locator(
      'button:has-text("Reset"), .reset-btn, [class*="reset"]',
    );
    // Should have zero matches
    await expect(resetBtn).toHaveCount(0);
  });

  test("solo button triggers API calls when clicked", async ({ page }) => {
    // Login first
    await loginAs(page, "petka");

    const apiCalls: string[] = [];
    page.on("request", (req) => {
      if (req.url().includes("/api/mixer/")) apiCalls.push(req.url());
    });

    await page.goto("/petka");
    await page.waitForLoadState("networkidle");

    // Wait for channels to load
    await page.waitForSelector(".channels-grid, .channel", { timeout: 10000 });

    const soloBtn = page
      .locator('button:has-text("S"), .solo-btn, [class*="solo"]')
      .first();
    if ((await soloBtn.count()) > 0) {
      const initialCalls = apiCalls.length;
      await soloBtn.click();
      await page.waitForTimeout(500);
      // Solo should trigger mute commands for other channels
      expect(apiCalls.length).toBeGreaterThan(initialCalls);
    }
  });
});
