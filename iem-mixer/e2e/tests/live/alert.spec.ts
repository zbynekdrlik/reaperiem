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

test.describe("Band Member Alert Button (#125)", () => {
  test("band member sees alert button on mixer page", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    await waitForMixer(page);

    // Alert button should be visible for band members
    const alertBtn = page.locator(".alert-btn");
    await expect(alertBtn).toBeVisible({ timeout: 5000 });
  });

  test("engineer does NOT see alert button", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    // Engineer should NOT have an alert button
    const alertBtn = page.locator(".alert-btn");
    await expect(alertBtn).toHaveCount(0);
  });

  test("alert button shows active state after click (no countdown)", async ({
    page,
  }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const alertBtn = page.locator(".alert-btn");
    await expect(alertBtn).toBeVisible({ timeout: 5000 });

    // Click SOS
    await alertBtn.click({ force: true });

    // Button should show active state (not disabled, has "active" class)
    // Poll until active class appears — WebSocket round-trip takes time on live system
    await expect(alertBtn).toHaveClass(/active/, { timeout: 15000 });
  });

  test("alert persists until engineer dismisses", async ({ browser }) => {
    const ctx1 = await browser.newContext();
    const ctx2 = await browser.newContext();
    const memberPage = await ctx1.newPage();
    const engineerPage = await ctx2.newPage();

    await memberPage.goto("/");
    const membersResp = await memberPage.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0];
    await loginAs(memberPage, member.id);
    await memberPage.goto(`/${member.id}`);
    await waitForMixer(memberPage);

    await engineerPage.goto("/");
    await loginAs(engineerPage, "engineer", "1177");
    await engineerPage.goto("/engineer");
    await waitForMixer(engineerPage);

    await memberPage.waitForTimeout(1000);
    await engineerPage.waitForTimeout(1000);

    // Member clicks SOS
    const alertBtn = memberPage.locator(".alert-btn");
    await expect(alertBtn).toBeVisible({ timeout: 5000 });
    await alertBtn.click({ force: true });

    // Engineer sees toast
    const toast = engineerPage.locator(".alert-toast");
    await expect(toast).toBeVisible({ timeout: 5000 });

    // Wait 6 seconds — toast must STILL be visible (no auto-dismiss)
    await engineerPage.waitForTimeout(6000);
    await expect(toast).toBeVisible();

    // Engineer dismisses
    const dismissBtn = engineerPage.locator(".alert-toast-dismiss");
    await dismissBtn.click({ force: true });

    // Toast disappears
    await expect(toast).not.toBeVisible({ timeout: 3000 });

    // Member button returns to idle (not active)
    await memberPage.waitForTimeout(1000);
    const memberBtnClass = await alertBtn.getAttribute("class");
    expect(memberBtnClass).not.toContain("active");

    await ctx1.close();
    await ctx2.close();
  });
});
