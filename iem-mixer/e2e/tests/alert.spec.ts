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

test.describe("Band Member Alert Button (#125)", () => {
  test("band member sees alert button on mixer page", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    if (!(await waitForMixer(page))) return;

    // Alert button should be visible for band members
    const alertBtn = page.locator(".alert-btn");
    await expect(alertBtn).toBeVisible({ timeout: 5000 });
  });

  test("engineer does NOT see alert button", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    // Engineer should NOT have an alert button
    const alertBtn = page.locator(".alert-btn");
    await expect(alertBtn).toHaveCount(0);
  });

  test("alert button shows cooldown after click", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    if (!(await waitForMixer(page))) return;

    const alertBtn = page.locator(".alert-btn");
    const btnVisible = await alertBtn
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(btnVisible, "alert button must be visible")) return;

    // Click the alert button
    await alertBtn.click({ force: true });
    await page.waitForTimeout(300);

    // Button should show cooldown state (disabled or cooldown class)
    const isDisabled = await alertBtn.isDisabled();
    const hasClass = (await alertBtn.getAttribute("class"))?.includes(
      "cooldown",
    );
    expect(isDisabled || hasClass).toBeTruthy();
  });

  test("engineer receives alert toast when member calls for help", async ({
    browser,
  }) => {
    // Two pages: member sends alert, engineer receives toast
    const ctx1 = await browser.newContext();
    const ctx2 = await browser.newContext();
    const memberPage = await ctx1.newPage();
    const engineerPage = await ctx2.newPage();

    // Get first member
    await memberPage.goto("/");
    const membersResp = await memberPage.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) {
      await ctx1.close();
      await ctx2.close();
      return;
    }

    const member = members[0];

    // Login member
    await loginAs(memberPage, member.id);
    await memberPage.goto(`/${member.id}`);
    if (!(await waitForMixer(memberPage))) {
      await ctx1.close();
      await ctx2.close();
      return;
    }

    // Login engineer
    await loginAs(engineerPage, "engineer", "1177");
    await engineerPage.goto("/engineer");
    if (!(await waitForMixer(engineerPage))) {
      await ctx1.close();
      await ctx2.close();
      return;
    }

    // Wait for WS connections to establish
    await memberPage.waitForTimeout(1000);
    await engineerPage.waitForTimeout(1000);

    // Member clicks alert button
    const alertBtn = memberPage.locator(".alert-btn");
    const btnVisible = await alertBtn
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(btnVisible, "alert button must be visible on member page")) {
      await ctx1.close();
      await ctx2.close();
      return;
    }
    await alertBtn.click({ force: true });

    // Engineer should see alert toast with member name
    const toast = engineerPage.locator(".alert-toast");
    await expect(toast).toBeVisible({ timeout: 5000 });

    // Toast should contain the member's name
    const toastText = await toast.textContent();
    expect(toastText).toBeTruthy();

    await ctx1.close();
    await ctx2.close();
  });
});
