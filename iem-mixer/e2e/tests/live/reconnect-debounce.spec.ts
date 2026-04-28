import { test, expect, Page } from "@playwright/test";

/**
 * Issue #186 — verify the 3s client-side debounce on the
 * "Reconnecting to REAPER..." banner.
 *
 * The banner only shows when the WebSocket has been disconnected for >=3s
 * AND the mixer is past initial load. This requires a live server with a
 * working REAPER backend (so `loading` transitions to false on snapshot),
 * which is why this test lives under `live/` and runs on the post-deploy
 * E2E job, not the CI E2E job.
 */

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

async function waitForMixerLoaded(page: Page): Promise<void> {
  // The .app.mixer container renders once Leptos mounts; wait for the
  // first channel-strip node to confirm `loading=false` (server has
  // delivered the initial Snapshot).
  await expect(
    page.locator(".app.mixer, .mixer-header").first(),
  ).toBeVisible({ timeout: 15000 });
  // Loading spinner is gated by `!loading`; once any channel strip is
  // visible the loading state is past.
  await expect(
    page.locator(".disconnected-banner"),
  ).not.toBeVisible();
}

test.describe("Reconnect banner debounce (#186)", () => {
  test("transient offline (<3s) does NOT show 'Reconnecting' banner", async ({
    context,
    page,
  }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        // Known-benign browser notices — mirrors backup-cg-remute /
        // snapshot-isolation filters.
        if (text.includes("apple-mobile-web-app-capable")) return;
        if (text.includes("[push] subscribe await failed")) return;
        if (text.includes("Push API in incognito mode")) return;
        if (/integrity.*attribute.*ignored/i.test(text)) return;
        if (text.includes("vapid-key fetch error")) return;
        consoleErrors.push(`[${msg.type()}] ${text}`);
      }
    });

    // Navigate to root before loginAs — Playwright starts on about:blank,
    // and `localStorage.setItem` (inside loginAs) is denied on that origin.
    // Mirrors the pattern in mixer.spec.ts.
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    await waitForMixerLoaded(page);

    // Force-close the existing WebSocket. context.setOffline alone does NOT
    // close existing WebSockets in Chromium — it only blocks new connection
    // attempts. The WS is exposed on window as `__iem_ws` for exactly this
    // kind of test introspection (connection.rs sets it during connect).
    await page.evaluate(() => {
      const ws = (window as unknown as { __iem_ws?: WebSocket }).__iem_ws;
      if (ws) ws.close();
    });

    // Reconnect closure ticks every 2s. Since we're online, the 2s reconnect
    // attempt should succeed and snapshot-restore connected=true BEFORE the
    // 3s debounce fires — Effect cancels the timer and banner stays hidden.

    // 1s in: well within debounce window, no reconnect yet.
    await page.waitForTimeout(1000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // 2s in: reconnect closure may have fired and reconnected by now.
    await page.waitForTimeout(1000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // 4s total elapsed (well past 3s debounce mark) — banner must STILL be
    // hidden because reconnect cancelled the timer before it fired.
    await page.waitForTimeout(2000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    expect(consoleErrors).toEqual([]);
  });

  test("sustained offline (>3s) DOES show banner; hides on reconnect", async ({
    context,
    page,
  }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        // Known-benign browser notices — mirrors backup-cg-remute /
        // snapshot-isolation filters.
        if (text.includes("apple-mobile-web-app-capable")) return;
        if (text.includes("[push] subscribe await failed")) return;
        if (text.includes("Push API in incognito mode")) return;
        if (/integrity.*attribute.*ignored/i.test(text)) return;
        if (text.includes("vapid-key fetch error")) return;
        consoleErrors.push(`[${msg.type()}] ${text}`);
      }
    });

    // Navigate to root before loginAs — Playwright starts on about:blank,
    // and `localStorage.setItem` (inside loginAs) is denied on that origin.
    // Mirrors the pattern in mixer.spec.ts.
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    await waitForMixerLoaded(page);

    // setOffline blocks new WS connections (so reconnect attempts fail).
    // Order matters: setOffline FIRST, then close the existing WS — otherwise
    // the reconnect closure could open a new WS in the gap before setOffline
    // takes effect.
    await context.setOffline(true);
    await page.evaluate(() => {
      const ws = (window as unknown as { __iem_ws?: WebSocket }).__iem_ws;
      if (ws) ws.close();
    });

    // 2s in — banner stays hidden inside debounce window. Reconnect attempts
    // happen at the 2s tick but fail (offline), re-firing connected=false.
    // The sticky-timer fix means the in-flight 3s timer is NOT restarted.
    await page.waitForTimeout(2000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // After 4s total disconnected the 3s debounce has fired — banner visible.
    await page.waitForTimeout(2000);
    await expect(page.locator(".disconnected-banner")).toBeVisible();

    // Restore network — banner clears as soon as the WebSocket reconnects
    // and the server delivers a fresh Snapshot.
    await context.setOffline(false);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible({
      timeout: 10000,
    });

    expect(consoleErrors).toEqual([]);
  });
});
