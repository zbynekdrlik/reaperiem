import { test, expect, Page, ConsoleMessage } from "@playwright/test";

/**
 * Post-deploy E2E for the navigation-back disposal race (#153 follow-up).
 *
 * Reproduces the user-reported Android PWA bug: after navigating back from
 * the member mixer to the member selector, the panic hook shows an error
 * page with "tried to access a reactive value that has already been
 * disposed". The underlying cause was plain `.set()` / `.update()` calls on
 * Leptos signals racing with component disposal. v1.144.0 hardened this by
 * converting every signal write to its `try_` variant and adding a
 * context-free CI scanner.
 *
 * Oracles (all three must pass for each scenario):
 *   1. No panic overlay is visible on the landing page after navigation.
 *   2. No `console.error` / `console.warn` messages during navigation
 *      and the 3-second settling window (matches the repo-wide
 *      browser-console-zero-errors contract).
 *   3. No POST to /api/client-error hits the server during the settling
 *      window (intercepted via page.route).
 *
 * Settling window: 3 seconds. The production logs showed the disposal
 * panic arriving at ~1 Hz (~2 full reconnect-interval ticks over 3s), so
 * 3s catches any leftover ghost interval that survived teardown.
 */

async function loginAs(page: Page, member: string, pin: string): Promise<void> {
  const response = await page.request.post("/api/auth", {
    data: { member, pin },
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

async function waitForStreamingMixer(page: Page): Promise<void> {
  // Mixer header visible (component mounted)
  await expect(page.locator(".mixer-header").first()).toBeVisible({
    timeout: 15000,
  });
  // Disconnected banner NOT visible (WebSocket is open)
  await expect(page.locator(".disconnected-banner")).not.toBeVisible({
    timeout: 15000,
  });
  // At least one channel rendered (State message arrived).
  // The repo-wide convention for channel elements is `.channel` (not
  // `.channel-strip`) — see mixer.spec.ts for prior art.
  await expect(page.locator(".channel").first()).toBeVisible({
    timeout: 15000,
  });
  // Give the WebSocket a beat to receive a Meters frame so onmessage
  // has actually run at least once before we navigate away.
  await page.waitForTimeout(500);
}

function collectConsoleNoise(page: Page): { errors: string[]; warnings: string[] } {
  const errors: string[] = [];
  const warnings: string[] = [];
  page.on("console", (msg: ConsoleMessage) => {
    const type = msg.type();
    if (type === "error") errors.push(msg.text());
    else if (type === "warning") warnings.push(msg.text());
  });
  return { errors, warnings };
}

async function interceptClientErrorPosts(page: Page): Promise<string[]> {
  const posts: string[] = [];
  await page.route("**/api/client-error", async (route) => {
    const req = route.request();
    if (req.method() === "POST") {
      posts.push(req.postData() ?? "(empty)");
    }
    // Let the request continue so we also exercise the real endpoint.
    await route.continue();
  });
  return posts;
}

async function assertNoPanicOverlay(page: Page): Promise<void> {
  // The panic overlay injected by lifecycle::build_overlay_html contains a
  // known headline. If this selector is visible, the panic hook fired.
  const overlay = page.locator('body:has-text("Something went wrong")');
  await expect(overlay).toHaveCount(0);
}

// Pre-existing non-disposal noise that Playwright's chromium produces in
// its incognito profile and that the production code cannot silence.
// These are NOT caused by the disposal race; filter them out of the
// oracle so the test fails ONLY on disposal-specific noise.
const NON_DISPOSAL_NOISE_PATTERNS = [
  // Chrome hard-blocks the Push API in incognito; subscribe_to_push()
  // (engineer-only, one-shot on mount) prints this warning. Unrelated
  // to #153 — tracked by Chrome bug 401439.
  /Push API in incognito mode/i,
  // Engineer's subscribe_to_push() logs this when Chromium blocks the
  // Push subscribe await. Same root cause as "Push API in incognito";
  // not disposal-related.
  /\[push\] subscribe await failed/i,
  // Chromium preload asset warning — the integrity attribute on a
  // preload link is ignored in some contexts. Not disposal-related.
  /integrity.*attribute.*ignored/i,
];

function filterKnownNoise(messages: string[]): string[] {
  return messages.filter(
    (m) => !NON_DISPOSAL_NOISE_PATTERNS.some((re) => re.test(m)),
  );
}

async function assertCleanSettle(
  page: Page,
  console_noise: { errors: string[]; warnings: string[] },
  client_error_posts: string[],
  settleMs: number,
): Promise<void> {
  await page.waitForTimeout(settleMs);
  await assertNoPanicOverlay(page);

  const disposedNoise = [...console_noise.errors, ...console_noise.warnings].filter(
    (m) => m.toLowerCase().includes("disposed"),
  );
  expect(
    disposedNoise,
    `console had disposed-signal messages: ${JSON.stringify(disposedNoise)}`,
  ).toEqual([]);

  expect(
    client_error_posts,
    `unexpected /api/client-error POSTs during settle: ${JSON.stringify(
      client_error_posts,
    )}`,
  ).toEqual([]);

  const filteredErrors = filterKnownNoise(console_noise.errors);
  const filteredWarnings = filterKnownNoise(console_noise.warnings);
  expect(
    filteredErrors,
    `unexpected console errors during settle: ${JSON.stringify(filteredErrors)}`,
  ).toEqual([]);
  expect(
    filteredWarnings,
    `unexpected console warnings during settle: ${JSON.stringify(filteredWarnings)}`,
  ).toEqual([]);
}

test.describe("Navigation-back disposal race (#153 follow-up)", () => {
  test("mixer → landing via browser back: no panic, no console noise", async ({
    page,
  }) => {
    const noise = collectConsoleNoise(page);
    const posts = await interceptClientErrorPosts(page);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForStreamingMixer(page);

    // Navigate back via the browser's history API (matches Android back button).
    // We deliberately do NOT assert on the final URL: landing.rs has an
    // auto-redirect effect that sends authenticated users straight to
    // `/<member>` once per session, so the final URL is non-deterministic.
    // What matters is that the mixer's disposal and whatever landing/redirect
    // transition follows produces zero panics, zero console noise, and zero
    // /api/client-error POSTs — which is exactly what the oracles check.
    await page.goBack();

    await assertCleanSettle(page, noise, posts, 3000);
  });

  test("mixer → landing via in-page back button: no panic", async ({
    page,
  }) => {
    const noise = collectConsoleNoise(page);
    const posts = await interceptClientErrorPosts(page);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForStreamingMixer(page);

    // The in-page back button is the .back-btn in mixer-header.
    await page.locator(".back-btn").click();

    await assertCleanSettle(page, noise, posts, 3000);
  });

  test("mixer → different member's mixer: no panic", async ({ page }) => {
    const noise = collectConsoleNoise(page);
    const posts = await interceptClientErrorPosts(page);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForStreamingMixer(page);

    // Engineer can navigate directly to another member's mixer by URL.
    await page.goto("/petronela");
    await waitForStreamingMixer(page);

    await assertCleanSettle(page, noise, posts, 3000);
  });

  test("mixer → landing → mixer loop, 3 iterations: no panic accumulates", async ({
    page,
  }) => {
    const noise = collectConsoleNoise(page);
    const posts = await interceptClientErrorPosts(page);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");

    for (let i = 0; i < 3; i++) {
      await page.goto("/engineer");
      await waitForStreamingMixer(page);
      await page.goBack();
      // Short settle between iterations; the final 3s settle is at the end.
      // URL after goBack is non-deterministic due to landing.rs auto-redirect —
      // the oracle is the disposal noise check, not the URL.
      await page.waitForTimeout(500);
    }

    await assertCleanSettle(page, noise, posts, 3000);
  });
});
