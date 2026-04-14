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

// tone_generator.lua is registered in reaper-kb.ini with a SHA-based action ID
// (REAPER computes it from the script path). Only this named ID fires the
// script via HTTP. The dynamic numeric IDs written to EXTSTATE by meter_bridge
// via reaper.AddRemoveReaScript() do NOT fire the script via the HTTP API —
// they're an internal command registry that is separate from HTTP-addressable
// actions. CLAUDE.md's "_RS_REAPERIEM_TONE_GEN" shorthand is wrong; the real
// ID is the one in reaper-kb.ini.
//
// If tone_generator.lua's path ever changes, this hash will change — update
// it by grepping reaper-kb.ini for "tone_generator.lua" on iem.lan.
const TONE_GEN_ACTION_ID =
  "_RS39188b77eebdd3fcf32cb58063b41d1e3828b057";

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

async function setToneGenerator(page: Page, on: boolean): Promise<void> {
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
    .get(`${REAPER_URL}/_/${TONE_GEN_ACTION_ID}`)
    .catch(() => {});
  // Tone insert/remove + mute toggling needs ~300 ms to stabilise.
  await page.waitForTimeout(300);
  // Verify the script actually ran — its atexit line clears tone_gen_action.
  const resp = await page.request.get(
    `${REAPER_URL}/_/GET/EXTSTATE/reaperiem/tone_gen_action`,
  );
  const body = await resp.text();
  const remaining = body.split("\t")[3]?.trim() || "";
  if (remaining !== "") {
    throw new Error(
      `tone_generator did not run: tone_gen_action still '${remaining}'. Check action ID hash.`,
    );
  }
}

async function readActiveText(page: Page): Promise<string> {
  const label = page.locator(".limiter-activity-label");
  await expect(label).toBeVisible({ timeout: 5000 });
  return (await label.innerText()).trim();
}

function parseActiveSeconds(text: string): number {
  // format_active() output:
  //   "not limited yet"              → 0
  //   "21.3 sec limited"             → 21.3
  //   "1 min 23 sec limited"         → 83.0
  const t = text.trim();
  if (t === "not limited yet") return 0;
  const secsOnly = t.match(/^(\d+(?:\.\d+)?)\s+sec\s+limited$/);
  if (secsOnly) return parseFloat(secsOnly[1]);
  const minSec = t.match(/^(\d+)\s+min\s+(\d+)\s+sec\s+limited$/);
  if (minSec) return parseInt(minSec[1], 10) * 60 + parseInt(minSec[2], 10);
  throw new Error(`Unparseable limiter-activity text: '${text}'`);
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
      `After Reset + tone-off + reopen, expected 'not limited yet' or '<=1.0 sec limited', got '${afterReset}'`,
    ).toBeLessThanOrEqual(1);

    // Cleanly close before afterEach runs the console-error check.
    await page.locator(".limiter-close-btn").click();
  });
});
