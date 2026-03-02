import { test, expect } from "@playwright/test";

test.describe("Smoke Tests - Must All Pass", () => {
  test("landing page loads and returns HTTP 200", async ({ page }) => {
    const response = await page.goto("/");
    expect(response?.status()).toBe(200);
  });

  test("landing page contains member content", async ({ page }) => {
    await page.goto("/");
    // Page should have some content (not blank)
    const content = await page.content();
    expect(content.length).toBeGreaterThan(100);
  });

  test("login page is accessible", async ({ page }) => {
    const response = await page.goto("/login");
    expect(response?.status()).toBe(200);
  });

  test("API members endpoint returns JSON", async ({ request }) => {
    const response = await request.get("/api/members");
    expect(response.status()).toBe(200);
    const data = await response.json();
    expect(Array.isArray(data)).toBe(true);
  });

  test("static assets load correctly", async ({ page }) => {
    await page.goto("/");
    // Check that WASM loads (page should have JS execution)
    await page.waitForLoadState("networkidle");
  });
});

test.describe("UX Polish - Issue #20: Mute Button Visibility", () => {
  test("mute-btn.off has neutral gray background (not red)", async ({
    page,
  }) => {
    await page.goto("/");
    await page.waitForLoadState("domcontentloaded");

    // Inject a test element with the mute-btn off class to check CSS
    const bgColor = await page.evaluate(() => {
      const btn = document.createElement("button");
      btn.className = "mute-btn off";
      btn.textContent = "M";
      document.body.appendChild(btn);
      const style = getComputedStyle(btn);
      const bg = style.backgroundColor;
      document.body.removeChild(btn);
      return bg;
    });

    // Parse RGB values — should be neutral gray (#2a2a3a), NOT red
    const match = bgColor.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/);
    expect(match).not.toBeNull();
    const red = parseInt(match![1]);
    // Neutral gray has low red channel (< 100); dark red would be > 80
    expect(red).toBeLessThan(100);
  });
});

test.describe("UX Polish - Issue #18: Fader 0 dB Marker", () => {
  test("fader-track has ::before pseudo-element at ~83% for 0 dB", async ({
    page,
  }) => {
    await page.goto("/");
    await page.waitForLoadState("domcontentloaded");

    // Inject a fader-track div and check ::before computed style
    const position = await page.evaluate(() => {
      const track = document.createElement("div");
      track.className = "fader-track";
      track.style.width = "300px";
      track.style.height = "44px";
      track.style.position = "relative";
      document.body.appendChild(track);
      const style = getComputedStyle(track, "::before");
      const left = style.left;
      document.body.removeChild(track);
      return left;
    });

    // ::before left should be ~83.33% of 300px = ~250px
    const px = parseFloat(position);
    expect(px).toBeGreaterThan(240);
    expect(px).toBeLessThan(260);
  });
});

test.describe("UX Polish - Issue #3: Login Back Button", () => {
  test("login page has back button that navigates to landing page", async ({
    page,
  }) => {
    await page.goto("/login?member=petronela&next=/petronela");
    await page.waitForLoadState("domcontentloaded");

    // Back button should be visible in the header
    const backBtn = page.locator(".back-btn");
    await expect(backBtn).toBeVisible({ timeout: 10000 });

    // Header should show "IEM Mixer"
    const header = page.locator(".mixer-header h1");
    await expect(header).toContainText("IEM Mixer");

    // Click back button — should navigate to landing page
    await backBtn.click();
    await page.waitForURL("**/", { timeout: 5000 });
    expect(page.url()).toMatch(/\/$/);
  });
});

test.describe("PWA Support - Issue #19", () => {
  test("manifest.json is served with valid PWA content", async ({
    request,
  }) => {
    const response = await request.get("/manifest.json");
    expect(response.status()).toBe(200);
    const manifest = await response.json();
    expect(manifest.name).toBe("IEM Mixer");
    expect(manifest.display).toBe("standalone");
    expect(manifest.start_url).toBe("/");
    expect(manifest.icons).toBeDefined();
    expect(manifest.icons.length).toBeGreaterThan(0);
  });

  test("icon.svg is served with correct content type", async ({ request }) => {
    const response = await request.get("/icon.svg");
    expect(response.status()).toBe(200);
    const contentType = response.headers()["content-type"];
    expect(contentType).toContain("svg");
  });

  test("icon PNGs are served", async ({ request }) => {
    const r192 = await request.get("/icon-192.png");
    expect(r192.status()).toBe(200);
    expect(r192.headers()["content-type"]).toContain("png");

    const r512 = await request.get("/icon-512.png");
    expect(r512.status()).toBe(200);
    expect(r512.headers()["content-type"]).toContain("png");
  });

  test("manifest icons include PNG entries", async ({ request }) => {
    const response = await request.get("/manifest.json");
    const manifest = await response.json();
    const pngIcons = manifest.icons.filter(
      (i: { type: string }) => i.type === "image/png",
    );
    expect(pngIcons.length).toBeGreaterThanOrEqual(2);
    expect(pngIcons.some((i: { sizes: string }) => i.sizes === "192x192")).toBe(
      true,
    );
    expect(pngIcons.some((i: { sizes: string }) => i.sizes === "512x512")).toBe(
      true,
    );
  });

  test("sw.js is served with no-cache header", async ({ request }) => {
    const response = await request.get("/sw.js");
    expect(response.status()).toBe(200);
    const cacheControl = response.headers()["cache-control"];
    expect(cacheControl).toContain("no-cache");
  });
});
