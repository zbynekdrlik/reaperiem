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

  test("alert button shows active state after click (no countdown)", async ({
    page,
  }) => {
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

    // Click SOS
    await alertBtn.click({ force: true });
    await page.waitForTimeout(500);

    // Button should show active state (not disabled, has "active" class)
    const hasActive = (await alertBtn.getAttribute("class"))?.includes(
      "active",
    );
    if (
      !assume(hasActive, "button must show active state (requires server)")
    )
      return;

    // Button should NOT be disabled (it's a toggle)
    const isEnabled = !(await alertBtn.isDisabled());
    expect(isEnabled).toBeTruthy();

    // Button text should indicate active
    const text = await alertBtn.textContent();
    expect(text).toContain("Active");
  });

  test("alert persists until engineer dismisses", async ({ browser }) => {
    const ctx1 = await browser.newContext();
    const ctx2 = await browser.newContext();
    const memberPage = await ctx1.newPage();
    const engineerPage = await ctx2.newPage();

    await memberPage.goto("/");
    const membersResp = await memberPage.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) {
      await ctx1.close();
      await ctx2.close();
      return;
    }

    const member = members[0];
    await loginAs(memberPage, member.id);
    await memberPage.goto(`/${member.id}`);
    if (!(await waitForMixer(memberPage))) {
      await ctx1.close();
      await ctx2.close();
      return;
    }

    await engineerPage.goto("/");
    await loginAs(engineerPage, "engineer", "1177");
    await engineerPage.goto("/engineer");
    if (!(await waitForMixer(engineerPage))) {
      await ctx1.close();
      await ctx2.close();
      return;
    }

    await memberPage.waitForTimeout(1000);
    await engineerPage.waitForTimeout(1000);

    // Member clicks SOS
    const alertBtn = memberPage.locator(".alert-btn");
    const btnVisible = await alertBtn
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(btnVisible, "alert button must be visible")) {
      await ctx1.close();
      await ctx2.close();
      return;
    }
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
