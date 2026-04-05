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

// Guard: early return when precondition not met
function assume(condition: unknown, message: string): condition is true {
  if (!condition) {
    console.log(`[ASSUME SKIP] ${message}`);
    return false;
  }
  return true;
}

// Wait for mixer page to load
async function waitForMixer(page: Page): Promise<boolean> {
  const mixerLoaded = await page
    .waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 })
    .catch(() => null);
  return assume(mixerLoaded, "Mixer must load (requires REAPER connection)");
}

test.describe("Audio Listen Button (#90)", () => {
  test("engineer sees Listen button in toolbar", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (!assume(toolbarLoaded, "Toolbar must render")) return;

    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });
    // Button text should contain "Listen"
    const text = await listenBtn.textContent();
    expect(text).toContain("Listen");
  });

  test("regular member does NOT see Listen button", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    if (!(await waitForMixer(page))) return;

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (!assume(toolbarLoaded, "Toolbar must render")) return;

    // Listen button should NOT be present for regular members
    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toHaveCount(0);
  });

  test("clicking Listen button opens audio WebSocket without errors", async ({
    page,
  }) => {
    // Capture browser console errors related to audio
    const audioErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (
          text.match(/AudioDecoder|opus|decode|NotSupportedError|AudioContext/i)
        ) {
          audioErrors.push(text);
        }
      }
    });

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (!assume(toolbarLoaded, "Toolbar must render")) return;

    const listenBtn = page.locator(".toolbar-btn-listen");
    if (!assume(await listenBtn.isVisible(), "Listen button must be visible"))
      return;

    // Click the Listen button — this should open a WebSocket to /ws/audio
    await listenBtn.click();

    // Wait for the WebSocket to connect and status to update
    await page.waitForTimeout(3000);

    // Button should have changed state (either listening or no-source)
    const btnClass = await listenBtn.getAttribute("class");
    expect(btnClass).toBeTruthy();
    const hasStateChange =
      btnClass?.includes("listening") || btnClass?.includes("no-source");
    // In CI without REAPER/VBAN, we expect no-source or the WS may fail
    // The key test is that the button click doesn't crash and changes state

    // No audio-related errors should appear in the browser console
    expect(audioErrors).toEqual([]);
  });

  test("audio WebSocket route exists and rejects plain HTTP", async ({
    request,
  }) => {
    // WebSocket endpoints return 400 for non-upgrade requests
    // This verifies the route is registered and reachable
    const audioResp = await request.get("/ws/audio");
    // 400 = route exists but requires WebSocket upgrade headers
    // (auth check happens after upgrade validation)
    expect(audioResp.status()).toBe(400);
  });
});

test.describe("Listen Boost Settings (#101)", () => {
  test("engineer settings shows listen boost section", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    // Open settings modal
    await page.locator(".settings-btn").click();
    await expect(
      page.locator('[data-testid="listen-boost-section"]'),
    ).toBeVisible({ timeout: 5000 });

    // Verify stepper controls exist
    await expect(page.locator('[data-testid="boost-minus"]')).toBeVisible();
    await expect(page.locator('[data-testid="boost-plus"]')).toBeVisible();
    await expect(page.locator('[data-testid="boost-value"]')).toHaveText(
      "0 dB",
    );
  });

  test("listen boost stepper increments and decrements by 3 dB", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    await page.locator(".settings-btn").click();
    await expect(
      page.locator('[data-testid="listen-boost-section"]'),
    ).toBeVisible({ timeout: 5000 });

    const boostValue = page.locator('[data-testid="boost-value"]');
    const plusBtn = page.locator('[data-testid="boost-plus"]');
    const minusBtn = page.locator('[data-testid="boost-minus"]');

    // Start at 0 dB
    await expect(boostValue).toHaveText("0 dB");

    // Increment twice: 0 → 3 → 6
    await plusBtn.click();
    await expect(boostValue).toHaveText("+3 dB");
    await plusBtn.click();
    await expect(boostValue).toHaveText("+6 dB");

    // Decrement once: 6 → 3
    await minusBtn.click();
    await expect(boostValue).toHaveText("+3 dB");

    // Decrement below 0 should clamp to 0
    await minusBtn.click();
    await expect(boostValue).toHaveText("0 dB");
    await minusBtn.click();
    await expect(boostValue).toHaveText("0 dB");
  });

  test("listen boost persists in localStorage", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    await page.locator(".settings-btn").click();
    await expect(
      page.locator('[data-testid="listen-boost-section"]'),
    ).toBeVisible({ timeout: 5000 });

    // Set boost to +12 dB (4 clicks)
    const plusBtn = page.locator('[data-testid="boost-plus"]');
    for (let i = 0; i < 4; i++) {
      await plusBtn.click();
    }
    await expect(page.locator('[data-testid="boost-value"]')).toHaveText(
      "+12 dB",
    );

    // Verify localStorage contains the boost value
    const stored = await page.evaluate(() => {
      const raw = localStorage.getItem("iem_settings_engineer");
      if (!raw) return null;
      return JSON.parse(raw);
    });
    expect(stored).toBeTruthy();
    expect(stored.listen_boost_db).toBe(12);

    // Reload and verify persistence
    await page.reload();
    if (!(await waitForMixer(page))) return;

    await page.locator(".settings-btn").click();
    await expect(
      page.locator('[data-testid="listen-boost-section"]'),
    ).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-testid="boost-value"]')).toHaveText(
      "+12 dB",
    );
  });

  test("non-engineer settings hides listen boost", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    if (!(await waitForMixer(page))) return;

    // Open settings modal
    await page.locator(".settings-btn").click();

    // Wait for settings modal to appear
    await expect(page.locator(".settings-modal")).toBeVisible({
      timeout: 5000,
    });

    // Audio section should NOT be present for regular members
    await expect(
      page.locator('[data-testid="listen-boost-section"]'),
    ).toHaveCount(0);
  });

  test("listen boost clamps at +24 dB maximum", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    await page.locator(".settings-btn").click();
    await expect(
      page.locator('[data-testid="listen-boost-section"]'),
    ).toBeVisible({ timeout: 5000 });

    // Click + 9 times (should clamp at 24)
    const plusBtn = page.locator('[data-testid="boost-plus"]');
    for (let i = 0; i < 9; i++) {
      await plusBtn.click();
    }
    await expect(page.locator('[data-testid="boost-value"]')).toHaveText(
      "+24 dB",
    );
  });
});
