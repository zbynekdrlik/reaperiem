/**
 * Limiter Tests — output bus limiter controls (#72).
 */

import { test, expect } from "@playwright/test";

const BASE_URL = process.env.E2E_BASE_URL || "http://localhost:80";

test.describe("Output Limiter — Issue #72", () => {
  test("engineer sees LIMIT button on mixer page", async ({
    page,
    request,
  }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    // Get first member for login
    const membersResp = await request.get(`${BASE_URL}/api/members`);
    const members = await membersResp.json();
    const member = members[0];

    // Login as engineer
    const loginResp = await request.post(`${BASE_URL}/api/auth`, {
      data: { member: member.id, pin: "1177" },
    });
    expect(loginResp.status()).toBe(200);
    const { token } = await loginResp.json();

    // Set auth in localStorage and navigate to mixer
    await page.goto(BASE_URL, { waitUntil: "networkidle" });
    await page.evaluate(
      ({ token, member }) => {
        localStorage.setItem(
          "iem_auth",
          JSON.stringify({ token, member: member.id, engineer: true }),
        );
      },
      { token, member },
    );
    await page.goto(`${BASE_URL}/${member.id}`, { waitUntil: "networkidle" });

    // Wait for mixer to render
    await page.waitForSelector(".channel-strip", { timeout: 15000 });

    // Check for LIMIT button (engineer-only)
    const limitBtn = page.locator(".limiter-btn-small");
    const count = await limitBtn.count();
    expect(count).toBeGreaterThanOrEqual(1);

    expect(consoleMessages).toEqual([]);
  });

  test("member does NOT see LIMIT button", async ({ page, request }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    const membersResp = await request.get(`${BASE_URL}/api/members`);
    const members = await membersResp.json();
    const member = members[0];

    // Login as regular member (not engineer)
    const loginResp = await request.post(`${BASE_URL}/api/auth`, {
      data: { member: member.id, pin: "7711" },
    });
    expect(loginResp.status()).toBe(200);
    const { token } = await loginResp.json();

    await page.goto(BASE_URL, { waitUntil: "networkidle" });
    await page.evaluate(
      ({ token, member }) => {
        localStorage.setItem(
          "iem_auth",
          JSON.stringify({ token, member: member.id, engineer: false }),
        );
      },
      { token, member },
    );
    await page.goto(`${BASE_URL}/${member.id}`, { waitUntil: "networkidle" });

    await page.waitForSelector(".channel-strip", { timeout: 15000 });

    // LIMIT button should NOT be visible to regular members
    const limitBtn = page.locator(".limiter-btn-small");
    const count = await limitBtn.count();
    expect(count).toBe(0);

    expect(consoleMessages).toEqual([]);
  });

  test("LIMIT button opens limiter modal", async ({ page, request }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    const membersResp = await request.get(`${BASE_URL}/api/members`);
    const members = await membersResp.json();
    const member = members[0];

    const loginResp = await request.post(`${BASE_URL}/api/auth`, {
      data: { member: member.id, pin: "1177" },
    });
    const { token } = await loginResp.json();

    await page.goto(BASE_URL, { waitUntil: "networkidle" });
    await page.evaluate(
      ({ token, member }) => {
        localStorage.setItem(
          "iem_auth",
          JSON.stringify({ token, member: member.id, engineer: true }),
        );
      },
      { token, member },
    );
    await page.goto(`${BASE_URL}/${member.id}`, { waitUntil: "networkidle" });

    await page.waitForSelector(".channel-strip", { timeout: 15000 });

    // Click LIMIT button
    const limitBtn = page.locator(".limiter-btn-small").first();
    await limitBtn.click();

    // Verify modal appears
    const modal = page.locator(".limiter-modal");
    await expect(modal).toBeVisible({ timeout: 5000 });

    // Verify modal has slider elements
    const sliders = page.locator(".limiter-slider-track");
    const sliderCount = await sliders.count();
    expect(sliderCount).toBe(3); // threshold, ceiling, release

    // Close modal
    const closeBtn = page.locator(".limiter-close-btn");
    await closeBtn.click();
    await expect(modal).not.toBeVisible({ timeout: 2000 });

    expect(consoleMessages).toEqual([]);
  });
});
