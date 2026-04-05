import { test, expect, Page } from "@playwright/test";

// Helper to login and set auth in localStorage
// Must navigate to app origin first so localStorage is accessible
async function loginAs(page: Page, member: string) {
  await page.goto("/");
  const response = await page.request.post("/api/auth", {
    data: { member, pin: "7711" },
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

test.describe("Issue #110 - Preset Input Accepts Digits and Backspace", () => {
  test("preset name input accepts digits and backspace after login", async ({
    page,
  }) => {
    // Check server is available
    const membersRes = await page.request.get("/api/members");
    expect(membersRes.ok()).toBe(true);
    const members = await membersRes.json();
    expect(members.length).toBeGreaterThan(0);

    const member = members[0].id;

    // Login via API and set token
    await loginAs(page, member);

    // Navigate to mixer page
    await page.goto(`/${member}`);

    // Wait for mixer to load
    await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible({ timeout: 10000 });

    // Click Presets button in toolbar
    const presetsBtn = page.locator(".toolbar-btn", { hasText: "Presets" });
    await expect(presetsBtn).toBeVisible({ timeout: 5000 });
    await presetsBtn.click();

    // Wait for preset modal and input to appear
    const presetInput = page.locator("input.preset-input");
    await expect(presetInput).toBeVisible({ timeout: 5000 });

    // Focus the input and type "Mix 123"
    await presetInput.click();
    await page.keyboard.type("Mix 123");

    // Verify input contains digits — this fails if global listener blocks them
    await expect(presetInput).toHaveValue("Mix 123");

    // Press Backspace 3 times to delete "123"
    await page.keyboard.press("Backspace");
    await page.keyboard.press("Backspace");
    await page.keyboard.press("Backspace");

    // Verify backspace worked — should be "Mix " (with trailing space)
    await expect(presetInput).toHaveValue("Mix ");
  });
});
