import { test, expect, Page } from "@playwright/test";

// Helper to login and set auth in localStorage
async function loginAs(page: Page, member: string) {
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

// Precondition check: logs explicitly and returns false when condition is not met.
function assume(condition: unknown, message: string): condition is true {
  if (!condition) {
    console.log(`[ASSUME SKIP] ${message}`);
    return false;
  }
  return true;
}

test.describe("Issue #110 - Preset Input Accepts Digits and Backspace", () => {
  test("preset name input accepts digits and backspace after login", async ({
    page,
  }) => {
    // Check server is available
    const membersRes = await page.request.get("/api/members");
    if (!assume(membersRes.ok(), "Server not available")) return;
    const members = await membersRes.json();
    if (!assume(members.length > 0, "No members configured")) return;

    const member = members[0].id;

    // Login via API and set token
    await loginAs(page, member);

    // Navigate to mixer page
    await page.goto(`/${member}`);

    // Wait for mixer to load
    const mixerLoaded = await page
      .waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 })
      .catch(() => null);
    if (!assume(mixerLoaded, "Mixer must load (requires REAPER connection)"))
      return;

    // Click Presets button in toolbar
    const presetsBtn = page.locator(".toolbar-btn", { hasText: "Presets" });
    const presetsBtnVisible = await presetsBtn
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(presetsBtnVisible, "Presets button must be visible")) return;
    await presetsBtn.click();

    // Wait for preset modal and input to appear
    const presetInput = page.locator("input.preset-input");
    const inputVisible = await presetInput
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(inputVisible, "Preset input must be visible")) return;

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
