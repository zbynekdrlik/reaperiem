import { test, expect, Page } from "@playwright/test";

/**
 * Issue #186 — smoke test for the 3s reconnect-banner debounce.
 *
 * NOTE on coverage: the timing-dependent "banner appears after 3s of
 * sustained disconnect" path is HARD to exercise reliably in Playwright
 * 1.42. `context.setOffline(true)` does not close existing WebSockets
 * in Chromium; it only blocks new connection attempts. `ws.close()` does
 * close the existing socket, but the project's reconnect closure runs
 * every 2 s and races with `setOffline` taking effect, leading to
 * unreliable timing. Playwright 1.48+ has `routeWebSocket` which would
 * solve this cleanly, but we are on 1.42.
 *
 * Coverage split:
 *   - Branch logic of `debounced_disconnect` is fully covered by the
 *     Rust unit test in `lifecycle.rs`
 *     (`debounced_disconnect_helper_branch_decisions`), including the
 *     sticky-timer regression guard.
 *   - This Playwright test is a smoke-level integration check: the
 *     helper compiles into the WASM bundle and the page loads with the
 *     wired `<Show>` branch evaluating to "hidden" while connected.
 *   - End-to-end disconnect-timing behavior is verified MANUALLY per
 *     the spec's Verification section (airplane-mode toggle on a
 *     phone).
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

test.describe("Reconnect banner debounce (#186)", () => {
  test("connected mixer never shows the 'Reconnecting' banner", async ({
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
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    // Mixer loaded → connected=true → banner must NOT be visible.
    await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible(
      { timeout: 15000 },
    );
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // Wait long enough that any spurious "schedule the timer" path inside
    // `debounced_disconnect` would have fired. While `connected = true`,
    // the helper must NEVER schedule the show transition — even after
    // multiple seconds of normal mixer activity (snapshot tick, meter
    // updates, etc.).
    await page.waitForTimeout(5000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    expect(consoleErrors).toEqual([]);
  });

  test("debounced_disconnect helper is wired into the page bundle", async ({
    page,
  }) => {
    // Compile-time integration check: the helper is reachable from
    // `crate::lifecycle::debounced_disconnect` and consumed by the
    // mixer page's `<Show>` block on `.disconnected-banner`. If the
    // helper fails to compile or the wiring is broken, the WASM bundle
    // will panic on mount and the panic-overlay (#iem-panic-overlay)
    // will replace the page body — see lifecycle.rs:install_panic_hook.
    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");

    await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible(
      { timeout: 15000 },
    );

    // Panic overlay must NOT be present — its presence proves the
    // helper or its consumers panicked at runtime.
    await expect(page.locator("#iem-panic-overlay")).not.toBeVisible();

    // The .disconnected-banner DOM node may or may not exist depending
    // on the `<Show>` evaluation; the only invariant we assert here is
    // that it is not currently visible (i.e., not currently rendered).
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();
  });
});
