/**
 * PWA Service Worker Tests — verify SW registers and caches hashed assets.
 *
 * The service worker caches content-hashed WASM/JS files (cache-first strategy)
 * for instant repeat loads. Only files matching /[a-f0-9]{16,}\.(js|wasm)$/ are
 * cached. index.html and unhashed files are NEVER cached in SW.
 *
 * Previous cache-ALL strategy caused blank pages after every deploy because
 * old WASM/JS assets were served from SW cache but didn't match new HTML references.
 * Current approach only caches immutable hashed files — safe across deploys.
 */

import { test, expect } from "@playwright/test";

const BASE_URL = process.env.E2E_BASE_URL || "http://localhost:8080";

test.describe("Service Worker — PWA with hashed asset caching", () => {
  test("service worker registers and activates", async ({ page }) => {
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
  });

  test("hashed WASM/JS assets are cached after navigation", async ({
    page,
  }) => {
    // Navigate to app
    await page.goto(BASE_URL, { waitUntil: "networkidle" });

    // Reload to ensure SW is active and can intercept fetches
    await page.reload({ waitUntil: "networkidle" });

    // Wait for SW to populate cache (may take a moment after activation)
    await page.waitForTimeout(2000);

    // Verify the iem-assets-v1 cache exists with hashed files
    const cacheInfo = await page.evaluate(async () => {
      const names = await caches.keys();
      if (!names.includes("iem-assets-v1")) return { exists: false, keys: [] };
      const cache = await caches.open("iem-assets-v1");
      const requests = await cache.keys();
      return {
        exists: true,
        keys: requests.map((r) => new URL(r.url).pathname),
      };
    });

    expect(cacheInfo.exists).toBe(true);
    // Should have cached at least the WASM and JS loader files
    const hashedFiles = cacheInfo.keys.filter(
      (k: string) => /[a-f0-9]{16,}\.(js|wasm)$/.test(k),
    );
    expect(hashedFiles.length).toBeGreaterThanOrEqual(1);

    // Verify unhashed files are NOT in cache
    const unhashedFiles = cacheInfo.keys.filter(
      (k: string) => !(/[a-f0-9]{16,}\.(js|wasm)$/.test(k)),
    );
    expect(unhashedFiles).toHaveLength(0);
  });
});
