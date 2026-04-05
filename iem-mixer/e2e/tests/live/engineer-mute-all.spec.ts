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

test.describe("Engineer Mute All (#88)", () => {
  test("engineer sees Mute All button in toolbar", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    const muteAllBtn = page.locator(".toolbar-btn-mute-all");
    await expect(muteAllBtn).toBeVisible({ timeout: 5000 });
    // Mute All button is now icon-only (🔇) — just verify it's visible
    expect(await muteAllBtn.textContent()).toBeTruthy();
  });

  test("regular member does NOT see Mute All button", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    const muteAllBtn = page.locator(".toolbar-btn-mute-all");
    await expect(muteAllBtn).toHaveCount(0);
  });

  test("Mute All API mutes all channels for engineer", async ({ request }) => {
    // Login as engineer
    const loginResp = await request.post("/api/auth", {
      data: { member: "engineer", pin: "1177" },
    });
    expect(loginResp.status()).toBe(200);
    const { token } = await loginResp.json();

    // Call batch mute_all
    const batchResp = await request.post("/api/mixer/engineer/batch", {
      headers: { Authorization: `Bearer ${token}` },
      data: { operation: "mute_all" },
    });
    expect(batchResp.ok()).toBe(true);

  test("clicking Mute All button fires batch API call", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    const muteAllBtn = page.locator(".toolbar-btn-mute-all");
    await expect(muteAllBtn).toBeVisible({ timeout: 5000 });

  test("Mute All hidden + Listen shown when engineer views member mixer", async ({
    page,
  }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    await loginAs(page, "engineer", "1177");
    await page.goto(`/${members[0].id}`);
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    // No Mute All on member's mixer
    await expect(page.locator(".toolbar-btn-mute-all")).toHaveCount(0);
    // Listen button should be visible (engineer can listen to member's mix)
    await expect(page.locator(".toolbar-btn-listen")).toBeVisible({
      timeout: 5000,
    });
  });

  test("engineer own mixer shows Mute All, no Presets/History", async ({
    page,
  }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    // Mute All visible
    await expect(page.locator(".toolbar-btn-mute-all")).toBeVisible();
    // No Presets or History
    await expect(
      page.locator(".toolbar-btn", { hasText: "Presets" }),
    ).toHaveCount(0);
    await expect(
      page.locator(".toolbar-btn", { hasText: "History" }),
    ).toHaveCount(0);
  });

  test("can unmute individual channel after Mute All", async ({ request }) => {
    // Login as engineer
    const loginResp = await request.post("/api/auth", {
      data: { member: "engineer", pin: "1177" },
    });
    expect(loginResp.status()).toBe(200);
    const { token } = await loginResp.json();

    // Mute all first
    const batchResp = await request.post("/api/mixer/engineer/batch", {
      headers: { Authorization: `Bearer ${token}` },
      data: { operation: "mute_all" },
    });
    expect(batchResp.ok()).toBe(true);
});
