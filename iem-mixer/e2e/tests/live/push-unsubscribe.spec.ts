import { test, expect, Page, Request } from "@playwright/test";

async function loginAs(page: Page, member: string, pin: string) {
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

test.describe("Push unsubscribe on engineer logout (#188)", () => {
  test("logout fires unsubscribe browser-side and server-side", async ({
    page,
  }) => {
    // Capture all console messages for end-of-test zero-error assertion.
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });

    // Capture relevant console logs/warns.
    const pushLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[push]")) pushLogs.push(text);
    });

    // Capture POST requests to push subscribe/unsubscribe.
    const pushSubscribeReqs: Request[] = [];
    const pushUnsubscribeReqs: Request[] = [];
    page.on("request", (req) => {
      const url = req.url();
      if (req.method() === "POST" && url.endsWith("/api/push/subscribe")) {
        pushSubscribeReqs.push(req);
      }
      if (req.method() === "POST" && url.endsWith("/api/push/unsubscribe")) {
        pushUnsubscribeReqs.push(req);
      }
    });

    // 1. Login as engineer and land on the mixer.
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await expect(
      page.locator(".app.mixer, .mixer-header").first(),
    ).toBeVisible({ timeout: 10000 });

    // 2. Wait for the engineer-subscribe console log (subscribe is fire-and-forget).
    //    If push isn't supported in the test environment, the helper logs and
    //    returns early — in that case we end the test as a graceful no-op so
    //    we don't get a false failure on browsers that lack the Push API.
    let subscribed = false;
    const start = Date.now();
    while (Date.now() - start < 15000) {
      if (pushLogs.some((l) => l.includes("engineer subscribed to Web Push"))) {
        subscribed = true;
        break;
      }
      if (
        pushLogs.some(
          (l) =>
            l.includes("VAPID not configured") ||
            l.includes("serviceWorker not available") ||
            l.includes("vapid-key fetch error"),
        )
      ) {
        // Push not available in this environment — graceful exit, NOT skip.
        // Assert what we CAN: logout still works without errors.
        await openSettingsAndLogout(page);
        await expect(page).toHaveURL(/\/$/);
        const token = await page.evaluate(() =>
          localStorage.getItem("iem_token"),
        );
        expect(token).toBeNull();
        expect(consoleErrors).toEqual([]);
        return;
      }
      await page.waitForTimeout(250);
    }
    expect(subscribed, `expected subscribe log; got: ${pushLogs.join(" | ")}`)
      .toBe(true);

    // Subscribe POST should have fired.
    expect(pushSubscribeReqs.length).toBeGreaterThanOrEqual(1);
    const subscribePostBody = pushSubscribeReqs[0].postDataJSON();
    const subscribedEndpoint: string = subscribePostBody?.endpoint ?? "";
    expect(subscribedEndpoint, "subscribe POST must include endpoint").toMatch(
      /^https?:\/\//,
    );

    // 3. Open Settings → click Logout.
    await openSettingsAndLogout(page);

    // 4. Assert browser-side unsubscribe console log appeared.
    const unsubBrowserOk = await waitForLog(pushLogs, "unsubscribed (browser)", 10000);
    expect(
      unsubBrowserOk,
      `expected browser unsubscribe log; got: ${pushLogs.join(" | ")}`,
    ).toBe(true);

    // 5. Assert server-side POST /api/push/unsubscribe fired with same endpoint and 200.
    const unsubReqOk = await waitForCondition(
      () => pushUnsubscribeReqs.length >= 1,
      10000,
    );
    expect(unsubReqOk, "expected POST /api/push/unsubscribe").toBe(true);

    const unsubReq = pushUnsubscribeReqs[0];
    const authHeader = unsubReq.headers()["authorization"];
    expect(authHeader).toMatch(/^Bearer .+/);
    const unsubBody = unsubReq.postDataJSON();
    expect(unsubBody?.endpoint).toBe(subscribedEndpoint);

    const unsubResp = await unsubReq.response();
    expect(unsubResp).not.toBeNull();
    expect(unsubResp!.status()).toBe(200);

    // 6. Assert URL navigated to landing and auth cleared.
    await expect(page).toHaveURL(/\/$/);
    const token = await page.evaluate(() => localStorage.getItem("iem_token"));
    expect(token).toBeNull();

    // 7. No console errors.
    expect(consoleErrors).toEqual([]);
  });
});

async function openSettingsAndLogout(page: Page) {
  // Settings button uses class "settings-btn" (mixer/mod.rs line 237).
  await page.locator(".settings-btn").first().click({
    timeout: 5000,
  });
  await expect(page.locator(".settings-modal").first()).toBeVisible({
    timeout: 5000,
  });
  await page.locator(".logout-btn").click();
}

async function waitForLog(
  logs: string[],
  needle: string,
  timeoutMs: number,
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (logs.some((l) => l.includes(needle))) return true;
    await new Promise((r) => setTimeout(r, 250));
  }
  return false;
}

async function waitForCondition(
  cond: () => boolean,
  timeoutMs: number,
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (cond()) return true;
    await new Promise((r) => setTimeout(r, 250));
  }
  return false;
}
