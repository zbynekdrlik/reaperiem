/**
 * PWA Service Worker Tests — verify SW registers and does NOT cache assets.
 *
 * The service worker exists solely for PWA installation (manifest, add-to-homescreen,
 * fullscreen mode). All asset caching is delegated to the browser's HTTP cache
 * combined with Trunk's content-hashed filenames.
 *
 * Previous cache-first SW strategy caused blank pages after every deploy because
 * old WASM/JS assets were served from SW cache but didn't match new HTML references.
 */

import { test, expect } from "@playwright/test";

const BASE_URL = process.env.E2E_BASE_URL || "http://localhost:8080";

test.describe("Service Worker — PWA without caching", () => {
  test("service worker registers and caches are empty after activation", async ({
    page,
  }) => {
    // Navigate to the app to trigger SW registration
    await page.goto(BASE_URL, { waitUntil: "networkidle" });

    // Wait for SW to register and activate
    const swRegistered = await page.evaluate(async () => {
      if (!("serviceWorker" in navigator)) return "unsupported";

      try {
        const reg = await navigator.serviceWorker.getRegistration();
        if (!reg) return "not-registered";

        // Wait for the SW to become active
        const sw = reg.active || reg.waiting || reg.installing;
        if (!sw) return "no-worker";

        if (sw.state !== "activated") {
          await new Promise<void>((resolve) => {
            sw.addEventListener("statechange", () => {
              if (sw.state === "activated") resolve();
            });
            // Resolve immediately if already activated
            if (sw.state === "activated") resolve();
          });
        }

        return "active";
      } catch (e) {
        return `error: ${e}`;
      }
    });

    if (swRegistered === "unsupported") {
      console.log("[SKIP] Service workers not supported in this browser");
      return;
    }

    expect(swRegistered).toBe("active");

    // Verify no caches exist (SW should have purged all caches on activate)
    const cacheNames = await page.evaluate(async () => {
      return await caches.keys();
    });

    expect(cacheNames).toHaveLength(0);
  });

  test("assets load from network, not service worker cache", async ({
    page,
  }) => {
    // Navigate to app
    await page.goto(BASE_URL, { waitUntil: "networkidle" });

    // Make a second navigation to ensure SW is active and could intercept
    await page.reload({ waitUntil: "networkidle" });

    // Verify caches are still empty after navigation
    const cacheNames = await page.evaluate(async () => {
      return await caches.keys();
    });

    expect(cacheNames).toHaveLength(0);
  });
});
