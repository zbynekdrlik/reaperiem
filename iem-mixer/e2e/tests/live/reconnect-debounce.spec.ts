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

    // Drop network — the WebSocket onclose fires, but the 3s debounce
    // should keep the banner hidden until the timer elapses.
    await context.setOffline(true);

    // 1s in: well within debounce window.
    await page.waitForTimeout(1000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // 2s in: still inside debounce window.
    await page.waitForTimeout(1000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // Restore network before the 3s threshold elapses (total offline ~= 2.2s).
    await context.setOffline(false);

    // Banner must NEVER appear during this transient blip — wait an
    // additional 2s (well past the 3s debounce mark from offline-start)
    // to be certain the timer didn't sneak through.
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

    await context.setOffline(true);

    // 1s, 2s — banner stays hidden inside debounce window.
    await page.waitForTimeout(2000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // After 4s total offline the 3s debounce has fired and the banner
    // is visible.
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
