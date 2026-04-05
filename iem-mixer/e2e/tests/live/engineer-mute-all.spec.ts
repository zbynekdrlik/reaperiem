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

// Wait for mixer page to load
async function waitForMixer(page: Page): Promise<void> {
  await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible({ timeout: 10000 });
}

test.describe("Engineer Mute All (#88)", () => {
  test("engineer sees Mute All button in toolbar", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    await expect(page.locator(".toolbar")).toBeVisible({ timeout: 10000 });

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

    await expect(page.locator(".toolbar")).toBeVisible({ timeout: 10000 });

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
    expect(batchResp.ok()).toBeTruthy();

    expect(batchResp.status()).toBe(200);

    // Verify all channels are muted by querying mixer state
    const mixerResp = await request.get("/api/mixer/engineer", {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(mixerResp.ok()).toBeTruthy();

    const data = await mixerResp.json();
    // ALL channels (input + mix) must be muted
    for (const ch of data.channels) {
      expect(ch.muted).toBe(true);
    }
  });

  test("clicking Mute All button fires batch API call", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    await expect(page.locator(".toolbar")).toBeVisible({ timeout: 10000 });

    const muteAllBtn = page.locator(".toolbar-btn-mute-all");
    await expect(muteAllBtn).toBeVisible();

    const requestPromise = page.waitForRequest(
      (req) =>
        req.url().includes("/api/mixer/engineer/batch") &&
        req.method() === "POST",
      { timeout: 5000 },
    );

    await muteAllBtn.click();

    const request = await requestPromise;
    const body = request.postDataJSON();
    expect(body.operation).toBe("mute_all");
  });

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

    await expect(page.locator(".toolbar")).toBeVisible({ timeout: 10000 });

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

    await expect(page.locator(".toolbar")).toBeVisible({ timeout: 10000 });

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
    expect(batchResp.ok()).toBeTruthy();

    // Get mixer state to find a track to unmute
    const mixerResp = await request.get("/api/mixer/engineer", {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(mixerResp.ok()).toBeTruthy();
    const data = await mixerResp.json();

    expect(data.channels.length).toBeGreaterThan(0);

    const firstTrack = data.channels[0].track_index;

    // Unmute the first channel
    const unmuteResp = await request.post(
      `/api/mixer/engineer/track/${firstTrack}/mute`,
      {
        headers: { Authorization: `Bearer ${token}` },
        data: { muted: false },
      },
    );
    expect(unmuteResp.ok()).toBeTruthy();

    // Verify: first channel unmuted, rest still muted
    const verifyResp = await request.get("/api/mixer/engineer", {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(verifyResp.ok()).toBeTruthy();
    const verifyData = await verifyResp.json();

    const firstChannel = verifyData.channels.find(
      (c: { track_index: number }) => c.track_index === firstTrack,
    );
    expect(firstChannel.muted).toBe(false);

    // At least some other channels should still be muted
    const stillMuted = verifyData.channels.filter(
      (c: { track_index: number; muted: boolean }) =>
        c.track_index !== firstTrack && c.muted,
    );
    expect(stillMuted.length).toBeGreaterThan(0);
  });
});
