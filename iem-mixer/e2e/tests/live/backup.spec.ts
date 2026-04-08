/**
 * Backup & Restore E2E Tests — verify backup API and Settings UI.
 *
 * Tests run against the deployed system on iem.lan with real REAPER.
 * Backup API is engineer-only (requires PIN 1177).
 */

import { test, expect, Page } from "@playwright/test";

/** Login helper — enters PIN via numpad */
async function loginAs(
  page: Page,
  member: string,
  pin: string = "7711",
): Promise<void> {
  await page.goto(`/login?member=${member}&next=/${member}`);
  await page.waitForLoadState("domcontentloaded");

  const numpad = page.locator(".numpad-btn");
  await expect(numpad.first()).toBeVisible({ timeout: 5000 });

  for (const digit of pin) {
    await numpad.filter({ hasText: digit }).click();
  }

  await page.waitForURL(`**/${member}`, { timeout: 10000 });
}

/** Get engineer auth token via API */
async function getEngineerToken(page: Page): Promise<string> {
  const resp = await page.request.post("/api/auth", {
    data: { member: "engineer", pin: "1177" },
  });
  expect(resp.status()).toBe(200);
  const data = await resp.json();
  expect(data.engineer).toBe(true);
  return data.token;
}

test.describe("Backup API (engineer-only)", () => {
  test("backup capture creates a backup file", async ({ page }) => {
    await page.goto("/");
    const token = await getEngineerToken(page);

    const resp = await page.request.post("/api/backups/capture", {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(resp.status()).toBe(200);

    const info = await resp.json();
    expect(info.filename).toBeTruthy();
    expect(info.filename).toContain(".json");
    expect(info.timestamp).toBeTruthy();
    expect(info.send_count).toBeGreaterThan(0);
    expect(info.track_count).toBeGreaterThan(0);
  });

  test("backup list returns available backups", async ({ page }) => {
    await page.goto("/");
    const token = await getEngineerToken(page);

    // Ensure at least one backup exists
    await page.request.post("/api/backups/capture", {
      headers: { Authorization: `Bearer ${token}` },
    });

    const resp = await page.request.get("/api/backups", {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(resp.status()).toBe(200);

    const list = await resp.json();
    expect(Array.isArray(list)).toBe(true);
    expect(list.length).toBeGreaterThan(0);
    expect(list[0].filename).toBeTruthy();
    expect(list[0].timestamp).toBeTruthy();
  });

  test("backup preview returns diff against live state", async ({ page }) => {
    await page.goto("/");
    const token = await getEngineerToken(page);

    // Capture a fresh backup
    const captureResp = await page.request.post("/api/backups/capture", {
      headers: { Authorization: `Bearer ${token}` },
    });
    const info = await captureResp.json();

    // Preview restore of the just-captured backup (should show mostly unchanged)
    const previewResp = await page.request.post(
      `/api/backups/${info.filename}/preview`,
      {
        headers: { Authorization: `Bearer ${token}` },
      },
    );
    expect(previewResp.status()).toBe(200);

    const preview = await previewResp.json();
    expect(preview).toHaveProperty("changes");
    expect(preview).toHaveProperty("unchanged_count");
    expect(preview).toHaveProperty("skipped");
    expect(Array.isArray(preview.changes)).toBe(true);
    expect(Array.isArray(preview.skipped)).toBe(true);
    // A fresh backup previewed immediately should have mostly unchanged values
    expect(preview.unchanged_count).toBeGreaterThan(0);
  });

  test("backup API rejects non-engineer access", async ({ page }) => {
    await page.goto("/");

    // Login as regular member
    const resp = await page.request.post("/api/auth", {
      data: { member: "petronela", pin: "7711" },
    });
    const data = await resp.json();

    // Try to list backups with member token
    const listResp = await page.request.get("/api/backups", {
      headers: { Authorization: `Bearer ${data.token}` },
    });
    expect(listResp.status()).toBe(403);
  });
});

test.describe("Backup UI in Settings modal", () => {
  test("backup section visible for engineer", async ({ page }) => {
    await loginAs(page, "engineer", "1177");

    // Open settings modal
    const settingsBtn = page.locator(".settings-btn");
    await expect(settingsBtn).toBeVisible({ timeout: 5000 });
    await settingsBtn.click();

    const settingsModal = page.locator(".settings-modal");
    await expect(settingsModal).toBeVisible({ timeout: 3000 });

    // Backup section should be visible
    const backupSection = page.locator('[data-testid="backup-section"]');
    await expect(backupSection).toBeVisible({ timeout: 3000 });
    await expect(backupSection).toContainText("Backups");
  });

  test("backup section NOT visible for regular member", async ({ page }) => {
    await loginAs(page, "petronela");

    // Open settings modal
    const settingsBtn = page.locator(".settings-btn");
    await expect(settingsBtn).toBeVisible({ timeout: 5000 });
    await settingsBtn.click();

    const settingsModal = page.locator(".settings-modal");
    await expect(settingsModal).toBeVisible({ timeout: 3000 });

    // Backup section should NOT be visible
    const backupSection = page.locator('[data-testid="backup-section"]');
    await expect(backupSection).not.toBeVisible();
  });
});
