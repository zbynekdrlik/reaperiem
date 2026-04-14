/**
 * Limiter Activity Counter — Issue #145
 *
 * Verifies that the per-track Active counter inside the LimiterModal
 * accumulates time when the safety limiter is reducing gain, and that
 * the Reset button zeros the counter (both server- and ReaScript-side).
 *
 * Requires REAPER on iem.lan with the modified MGA_JSLimiterST exposing
 * slider5 (deployed by CI).  Uses the tone_generator ReaScript to drive
 * a hot signal into the engineer's mix bus, which then exceeds the
 * limiter ceiling (-6 dBFS by default) on the engineer's inear track.
 */

import { test, expect, Page } from "@playwright/test";

const REAPER_URL = "http://iem.lan:8080";
const TONE_GEN_ACTION = "_RS_REAPERIEM_TONE_GEN";

async function loginAsEngineer(page: Page) {
  const response = await page.request.post("/api/auth", {
    data: { member: "engineer", pin: "1177" },
  });
  expect(response.status()).toBe(200);
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

async function setToneGenerator(page: Page, on: boolean) {
  // tone_generator.lua reads EXTSTATE reaperiem/tone_gen_action ("start"|"stop")
  // and acts accordingly. It is NOT a toggle — calling the action without
  // setting tone_gen_action first is a no-op that writes "ERROR:no_action".
  const action = on ? "start" : "stop";
  await page.request
    .get(
      `${REAPER_URL}/_/SET/EXTSTATE/reaperiem/tone_gen_action/${action}`,
    )
    .catch(() => {});
  await page.request
    .get(`${REAPER_URL}/_/${TONE_GEN_ACTION}`)
    .catch(() => {});
  // Tone insert/remove + mute toggling needs ~300 ms to stabilise.
  await page.waitForTimeout(300);
}

async function readActiveText(page: Page): Promise<string> {
  const label = page.locator(".limiter-activity-label");
  await expect(label).toBeVisible({ timeout: 5000 });
  return (await label.innerText()).trim();
}

function parseActiveSeconds(text: string): number {
  // Format: "Active: never" or "Active: M:SS"
  const stripped = text.replace(/^Active:\s*/, "").trim();
  if (stripped === "never") return 0;
  const match = stripped.match(/^(\d+):(\d{2})$/);
  if (!match) {
    throw new Error(`Unparseable active text: '${text}'`);
  }
  return parseInt(match[1], 10) * 60 + parseInt(match[2], 10);
}

test.describe("Limiter Activity Counter — Issue #145", () => {
  const consoleMessages: string[] = [];

  test.beforeEach(async ({ page }) => {
    consoleMessages.length = 0;
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        if (msg.text().includes("subscribe await failed")) return;
        if (msg.text().includes("Push API in incognito")) return;
        if (msg.text().includes("vapid-key fetch error")) return;
        if (msg.text().includes("navigator.vibrate")) return;
        if (msg.text().includes("closure invoked recursively")) return;
        if (msg.text().includes("[vite]")) return;
        if (msg.text().includes("favicon")) return;
        if (msg.text().includes("integrity")) return;
        if (msg.text().includes("WebSocket connection")) return;
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });
  });

  test.afterEach(async () => {
    expect(consoleMessages).toEqual([]);
  });

  test("counter accumulates while limiter is reducing gain, Reset zeros it", async ({
    page,
  }) => {
    test.setTimeout(60_000);

    // Login + navigate to engineer's own mixer
    await page.goto("/");
    await loginAsEngineer(page);
    await page.goto("/engineer");
    await expect(page.locator(".mixer-header").first()).toBeVisible({
      timeout: 10_000,
    });

    // Ensure tone is OFF at the start (stop is idempotent — stops whether or
    // not a tone_generator FX is currently inserted).
    await setToneGenerator(page, false);

    // The LIM button on the GlobalVolumeFader opens the limiter for the
    // logged-in member's OWN output track (ENGINEER inear when logged in
    // as engineer). The tone_generator drops its audio on the same track,
    // so the limiter on ENGINEER inear is the one that will engage.
    const limitBtn = page.locator(".limiter-btn-small").first();
    await expect(limitBtn).toBeVisible({ timeout: 10_000 });

    // Reset first so we measure ONLY this test's accumulation.
    await limitBtn.click();
    await expect(page.locator(".limiter-modal")).toBeVisible({ timeout: 5000 });
    const resetBtnPre = page.locator(".limiter-reset-btn");
    await expect(resetBtnPre).toBeVisible();
    await resetBtnPre.click();
    // Close + reopen so we re-fetch active_seconds from the server.
    await page.locator(".limiter-close-btn").click();
    await expect(page.locator(".limiter-modal")).not.toBeVisible({
      timeout: 2000,
    });

    // Turn the tone ON.
    await setToneGenerator(page, true);

    // Hold the hot signal long enough for the limiter to engage and accumulate
    // measurable active time.  meter_bridge polls per defer tick (~30 ms);
    // 6 s of audible signal should produce well over 5 s of accumulated activity.
    await page.waitForTimeout(6000);

    // Open the modal again and read the counter
    await limitBtn.click();
    await expect(page.locator(".limiter-modal")).toBeVisible({ timeout: 5000 });
    const activeText = await readActiveText(page);
    const activeSecs = parseActiveSeconds(activeText);
    expect(
      activeSecs,
      `Expected >= 5 s of limiter activity after a 6 s hot tone, got '${activeText}'`,
    ).toBeGreaterThanOrEqual(5);

    // Reset
    const resetBtn = page.locator(".limiter-reset-btn");
    await resetBtn.click();

    // Stop the tone before the next assertion, otherwise meter_bridge will
    // immediately accumulate again on the next tick and the counter will not
    // remain at zero.
    await setToneGenerator(page, false);

    // Close + reopen modal to re-fetch active_seconds from the server.
    await page.locator(".limiter-close-btn").click();
    await expect(page.locator(".limiter-modal")).not.toBeVisible({
      timeout: 2000,
    });
    await limitBtn.click();
    await expect(page.locator(".limiter-modal")).toBeVisible({ timeout: 5000 });

    const afterReset = await readActiveText(page);
    expect(
      parseActiveSeconds(afterReset),
      `After Reset + tone-off + reopen, expected 'Active: never' or 'Active: 0:00'-'0:01', got '${afterReset}'`,
    ).toBeLessThanOrEqual(1);

    // Cleanly close before afterEach runs the console-error check.
    await page.locator(".limiter-close-btn").click();
  });
});
