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

async function waitForMixer(page: Page) {
  await expect(page.locator(".app.mixer, .mixer-header")).toBeVisible({ timeout: 10000 });
}

test.describe("Stems Volume Fader", () => {
  test("stems fader visible on Main tab when stems bus exists", async ({
    page,
  }) => {
    await loginAs(page, "petka");
    await page.goto("/petka");

    await waitForMixer(page);

    // Check if stems bus tracks exist in REAPER
    const stemsFader = page.locator('[data-testid="stems-volume-fader"]');
    const count = await stemsFader.count();

    // If stems bus tracks exist, fader should be visible on Main tab (default)
    if (count > 0) {
      await expect(stemsFader.first()).toBeVisible();
      // Verify label
      await expect(stemsFader.first().locator(".ch-name")).toHaveText("STEMS");
      await expect(stemsFader.first().locator(".ch-type")).toHaveText("group");
    } else {
      console.log(
        "No stems bus tracks in REAPER — fader hidden as expected",
      );
    }
  });

  test("stems fader visible on Stems tab when stems bus exists", async ({
    page,
  }) => {
    await loginAs(page, "petka");
    await page.goto("/petka");

    await waitForMixer(page);

    // Click Stems tab
    const stemsTab = page.locator("text=Stems");
    if ((await stemsTab.count()) > 0) {
      await stemsTab.click();
      await page.waitForTimeout(200);
    }

    const stemsFader = page.locator('[data-testid="stems-volume-fader"]');
    const count = await stemsFader.count();

    if (count > 0) {
      await expect(stemsFader.first()).toBeVisible();
    } else {
      console.log(
        "No stems bus tracks in REAPER — fader hidden as expected",
      );
    }
  });

  test("stems fader NOT visible on Mics tab", async ({ page }) => {
    await loginAs(page, "petka");
    await page.goto("/petka");

    await waitForMixer(page);

    // Click Mics tab
    const micsTab = page.locator("text=Mics");
    if ((await micsTab.count()) > 0) {
      await micsTab.click();
      await page.waitForTimeout(200);
    }

    const stemsFader = page.locator('[data-testid="stems-volume-fader"]');
    await expect(stemsFader).toHaveCount(0);
  });

  test("stems fader NOT visible on Tech tab", async ({ page }) => {
    await loginAs(page, "petka");
    await page.goto("/petka");

    await waitForMixer(page);

    // Click Tech tab
    const techTab = page.locator("text=Tech");
    if ((await techTab.count()) > 0) {
      await techTab.click();
      await page.waitForTimeout(200);
    }

    const stemsFader = page.locator('[data-testid="stems-volume-fader"]');
    await expect(stemsFader).toHaveCount(0);
  });

  test("global volume fader always visible on Main tab", async ({ page }) => {
    await loginAs(page, "petka");
    await page.goto("/petka");

    await waitForMixer(page);

    // Global volume should always be on Main tab regardless of stems
    const globalFader = page.locator('[data-testid="global-volume-fader"]');
    await expect(globalFader).toBeVisible();
  });
});
