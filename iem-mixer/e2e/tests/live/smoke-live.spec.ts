import { test, expect } from "@playwright/test";

test.describe("Auth & Token Expiry - Issue #38", () => {
  test("settings modal has logout button", async ({ page }) => {
    // First authenticate
    await page.goto("/login?member=petronela&next=/petronela");
    await page.waitForLoadState("domcontentloaded");

    // Enter PIN (default 7711)
    const numpad = page.locator(".numpad-btn");
    await expect(numpad.first()).toBeVisible({ timeout: 5000 });

    await numpad.filter({ hasText: "7" }).click();
    await numpad.filter({ hasText: "7" }).click();
    await numpad.filter({ hasText: "1" }).click();
    await numpad.filter({ hasText: "1" }).click();

    // Should redirect to mixer page
    await page.waitForURL("**/petronela", { timeout: 5000 });

    // Open settings modal
    const settingsBtn = page.locator(".settings-btn");
    await expect(settingsBtn).toBeVisible({ timeout: 5000 });
    await settingsBtn.click();

    // Settings modal should be visible with logout button
    const settingsModal = page.locator(".settings-modal");
    await expect(settingsModal).toBeVisible({ timeout: 3000 });

    const logoutBtn = page.locator(".logout-btn");
    await expect(logoutBtn).toBeVisible();
    await expect(logoutBtn).toContainText("Logout");
  });

  test("logout clears auth and redirects to landing", async ({ page }) => {
    // First authenticate
    await page.goto("/login?member=petronela&next=/petronela");
    await page.waitForLoadState("domcontentloaded");

    // Enter PIN (default 7711)
    const numpad = page.locator(".numpad-btn");
    await expect(numpad.first()).toBeVisible({ timeout: 5000 });

    await numpad.filter({ hasText: "7" }).click();
    await numpad.filter({ hasText: "7" }).click();
    await numpad.filter({ hasText: "1" }).click();
    await numpad.filter({ hasText: "1" }).click();

    // Should redirect to mixer page
    await page.waitForURL("**/petronela", { timeout: 5000 });

    // Open settings modal
    const settingsBtn = page.locator(".settings-btn");
    await expect(settingsBtn).toBeVisible({ timeout: 5000 });
    await settingsBtn.click();

    // Click logout
    const logoutBtn = page.locator(".logout-btn");
    await expect(logoutBtn).toBeVisible({ timeout: 3000 });
    await logoutBtn.click();

    // Should redirect to landing page
    await page.waitForURL("**/", { timeout: 5000 });
    expect(page.url()).toMatch(/\/$/);

    // Auth should be cleared (localStorage)
    const token = await page.evaluate(() => localStorage.getItem("iem_token"));
    expect(token).toBeNull();
  });
});

test.describe("Snapshot History - Issue #46", () => {
  test("history button is visible in toolbar", async ({ page }) => {
    // First authenticate
    await page.goto("/login?member=petronela&next=/petronela");
    await page.waitForLoadState("domcontentloaded");

    // Enter PIN (default 7711)
    const numpad = page.locator(".numpad-btn");
    await expect(numpad.first()).toBeVisible({ timeout: 5000 });

    await numpad.filter({ hasText: "7" }).click();
    await numpad.filter({ hasText: "7" }).click();
    await numpad.filter({ hasText: "1" }).click();
    await numpad.filter({ hasText: "1" }).click();

    // Should redirect to mixer page
    await page.waitForURL("**/petronela", { timeout: 5000 });

    // History button should be visible in toolbar
    const historyBtn = page.locator(".toolbar-btn", { hasText: "History" });
    await expect(historyBtn).toBeVisible({ timeout: 5000 });
  });

  test("history button opens snapshot modal", async ({ page }) => {
    // First authenticate
    await page.goto("/login?member=petronela&next=/petronela");
    await page.waitForLoadState("domcontentloaded");

    // Enter PIN (default 7711)
    const numpad = page.locator(".numpad-btn");
    await expect(numpad.first()).toBeVisible({ timeout: 5000 });

    await numpad.filter({ hasText: "7" }).click();
    await numpad.filter({ hasText: "7" }).click();
    await numpad.filter({ hasText: "1" }).click();
    await numpad.filter({ hasText: "1" }).click();

    // Should redirect to mixer page
    await page.waitForURL("**/petronela", { timeout: 5000 });

    // Click History button
    const historyBtn = page.locator(".toolbar-btn", { hasText: "History" });
    await expect(historyBtn).toBeVisible({ timeout: 5000 });
    await historyBtn.click();

    // Snapshot modal should be visible
    const modal = page.locator(".snapshot-modal");
    await expect(modal).toBeVisible({ timeout: 3000 });
    await expect(modal.locator("h2")).toContainText("Mix History");
  });
});
