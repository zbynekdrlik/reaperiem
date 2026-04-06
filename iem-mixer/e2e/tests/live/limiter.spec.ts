/**
 * Limiter Tests — output bus limiter controls (#72).
 *
 * These tests verify the LIMIT button visibility and modal behavior.
 * The mixer page requires a REAPER connection to render channel strips,
 * so tests gracefully skip in CI when REAPER is not available.
 */

import { test, expect, Page } from "@playwright/test";

// Helper to login and set auth in localStorage (matches eq.spec.ts pattern)
async function loginAsEngineer(page: Page, member: string) {
  const response = await page.request.post("/api/auth", {
    data: { member, pin: "1177" },
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

async function loginAsMember(page: Page, member: string) {
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
  await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible({ timeout: 10000 });
}

test.describe("Output Limiter — Issue #72", () => {
  test("engineer sees LIMIT button on mixer page", async ({ page }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        if (msg.text().includes("subscribe await failed")) return;
        if (msg.text().includes("Push API in incognito")) return;
        if (msg.text().includes("navigator.vibrate")) return;
        if (msg.text().includes("closure invoked recursively")) return;
        if (msg.text().includes("[vite]")) return;
        if (msg.text().includes("favicon")) return;
        if (msg.text().includes("integrity")) return;
        if (msg.text().includes("WebSocket connection")) return;
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    // Get first member
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThan(0);
    const member = members[0];

    // Login as engineer and navigate
    await page.goto("/");
    await loginAsEngineer(page, "engineer");
    await page.goto(`/${member.id}`);

    await waitForMixer(page);

    // Wait for channel strips to appear (needs REAPER data)
    await expect(page.locator(".channel").first()).toBeVisible({ timeout: 10000 });

    // Check for LIMIT button (engineer-only)
    const limitBtn = page.locator(".limiter-btn-small");
    const count = await limitBtn.count();
    expect(count).toBeGreaterThanOrEqual(1);

    expect(consoleMessages).toEqual([]);
  });

  test("member does NOT see LIMIT button", async ({ page }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        if (msg.text().includes("subscribe await failed")) return;
        if (msg.text().includes("Push API in incognito")) return;
        if (msg.text().includes("navigator.vibrate")) return;
        if (msg.text().includes("closure invoked recursively")) return;
        if (msg.text().includes("[vite]")) return;
        if (msg.text().includes("favicon")) return;
        if (msg.text().includes("integrity")) return;
        if (msg.text().includes("WebSocket connection")) return;
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThan(0);
    const member = members[0];

    // Login as regular member
    await page.goto("/");
    await loginAsMember(page, member.id);
    await page.goto(`/${member.id}`);

    await waitForMixer(page);

    await expect(page.locator(".channel").first()).toBeVisible({ timeout: 10000 });

    // LIMIT button should NOT be visible to regular members
    const limitBtn = page.locator(".limiter-btn-small");
    const count = await limitBtn.count();
    expect(count).toBe(0);

    expect(consoleMessages).toEqual([]);
  });

  test("LIMIT button opens limiter modal", async ({ page }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        if (msg.text().includes("subscribe await failed")) return;
        if (msg.text().includes("Push API in incognito")) return;
        if (msg.text().includes("navigator.vibrate")) return;
        if (msg.text().includes("closure invoked recursively")) return;
        if (msg.text().includes("[vite]")) return;
        if (msg.text().includes("favicon")) return;
        if (msg.text().includes("integrity")) return;
        if (msg.text().includes("WebSocket connection")) return;
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThan(0);
    const member = members[0];

    await page.goto("/");
    await loginAsEngineer(page, "engineer");
    await page.goto(`/${member.id}`);

    await waitForMixer(page);

    await expect(page.locator(".channel").first()).toBeVisible({ timeout: 10000 });

    // Click LIMIT button
    const limitBtn = page.locator(".limiter-btn-small").first();
    await limitBtn.click();

    // Verify modal appears
    const modal = page.locator(".limiter-modal");
    await expect(modal).toBeVisible({ timeout: 5000 });

    // Verify modal has slider elements (wait for render)
    const sliders = page.locator(".limiter-slider-track");
    await expect(sliders.first()).toBeVisible({ timeout: 3000 });
    const sliderCount = await sliders.count();
    expect(sliderCount).toBe(1); // MAX LEVEL

    // Close modal
    const closeBtn = page.locator(".limiter-close-btn");
    await closeBtn.click();
    await expect(modal).not.toBeVisible({ timeout: 2000 });

    expect(consoleMessages).toEqual([]);
  });
});
