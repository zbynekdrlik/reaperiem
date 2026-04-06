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
  await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible({ timeout: 10000 });
}

test.describe("Engineer Talk Button", () => {
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
  });

  test.afterEach(async () => {
    const real = consoleMessages.filter(
      (m) =>
        !m.includes("[vite]") &&
        !m.includes("favicon") &&
        !m.includes("integrity") &&
        !m.includes("WebSocket connection") &&
        !m.includes("navigator.vibrate"),
    );
    expect(real).toEqual([]);
  });

  test("engineer sees talk button on own mixer page", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    const talkBtn = page.locator(".toolbar-btn-talk");
    await expect(talkBtn).toBeVisible({ timeout: 5000 });
  });

  test("band member does NOT see talk button", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const talkBtn = page.locator(".toolbar-btn-talk");
    await expect(talkBtn).toHaveCount(0);
  });

  test("engineer does NOT see talk button on member page", async ({
    page,
  }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const talkBtn = page.locator(".toolbar-btn-talk");
    await expect(talkBtn).toHaveCount(0);
  });
});
