import { test, expect, Page } from "@playwright/test";

// Helper to login and set auth in localStorage
async function loginAs(page: Page, member: string, pin: string = "7711") {
  const response = await page.request.post("/api/auth", {
    data: { member, pin },
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

async function waitForMixer(page: Page) {
  await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible({ timeout: 10000 });
}

test.describe("Band Member Alert Button (#125)", () => {
  test("band member sees alert button on mixer page", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    await waitForMixer(page);

    // Alert button should be visible for band members
    const alertBtn = page.locator(".alert-btn");
    await expect(alertBtn).toBeVisible({ timeout: 5000 });
  });

  test("engineer does NOT see alert button", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    // Engineer should NOT have an alert button
    const alertBtn = page.locator(".alert-btn");
    await expect(alertBtn).toHaveCount(0);
  });

  test("alert button shows active state after click (#150)", async ({ page }) => {
    // Capture only signal-bearing WebSocket frames for diagnosis on failure.
    // State and Meters frames carry full mix data (channel names, levels,
    // per-track meters) which is noise for this test and would also leak
    // private mixer state into CI logs. Everything we need for diagnosis —
    // whether CallEngineer went out, whether EngineerAlert came back,
    // whether AlertCleared was seen — is in the short signal frames.
    const SIGNAL_EVENTS = /"(event|cmd)"\s*:\s*"(CallEngineer|ClearAlert|EngineerAlert|ActiveAlerts|AlertCleared)"/;
    const wsSent: string[] = [];
    const wsReceived: string[] = [];
    page.on("websocket", (ws) => {
      if (!ws.url().includes("/ws/")) return;
      ws.on("framesent", (f) => {
        if (typeof f.payload === "string" && SIGNAL_EVENTS.test(f.payload)) {
          wsSent.push(f.payload);
        }
      });
      ws.on("framereceived", (f) => {
        if (typeof f.payload === "string" && SIGNAL_EVENTS.test(f.payload)) {
          wsReceived.push(f.payload);
        }
      });
    });

    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const alertBtn = page.locator(".alert-btn");
    await expect(alertBtn).toBeVisible({ timeout: 5000 });

    // Wait for the initial ServerMsg::State to arrive before clicking.
    // The click handler silently drops if `ws.ready_state() != OPEN`
    // (alert_button.rs:15). First channel rendering is a reliable proxy
    // for "WS open AND initial state delivered".
    await page.waitForFunction(
      () => document.querySelectorAll(".channel").length > 0,
      { timeout: 10000 },
    );

    // If the server holds a stale active alert for this member (prior
    // failed run), it broadcasts a catch-up EngineerAlert right after
    // initial State — the WS-connect handler added for #150. Poll for up
    // to 1s; if the button becomes active we clear it so the main flow
    // starts idle. If the button stays idle there is nothing to clear
    // and we proceed immediately — no fixed wait.
    let hadResidualAlert = false;
    try {
      await expect(alertBtn).toHaveClass(/active/, { timeout: 1000 });
      hadResidualAlert = true;
    } catch {
      // No residual — fresh state, proceed.
    }
    if (hadResidualAlert) {
      await alertBtn.click({ force: true });
      await expect(alertBtn).not.toHaveClass(/active/, { timeout: 5000 });
    }

    // Click SOS — send CallEngineer, expect `active` class from server echo.
    await alertBtn.click({ force: true });
    try {
      await expect(alertBtn).toHaveClass(/active/, { timeout: 10000 });
    } catch (err) {
      console.log("=== WS signal frames sent by member ===");
      for (const p of wsSent) console.log("→", p);
      console.log("=== WS signal frames received by member ===");
      for (const p of wsReceived) console.log("←", p);
      console.log(
        "=== alert-btn class at failure ===",
        await alertBtn.getAttribute("class"),
      );
      throw err;
    }

    // Clear the alert so we leave the server in a clean state.
    await alertBtn.click({ force: true });
    await expect(alertBtn).not.toHaveClass(/active/, { timeout: 5000 });
  });

  test("alert persists until engineer dismisses", async ({ browser }) => {
    const ctx1 = await browser.newContext();
    const ctx2 = await browser.newContext();
    const memberPage = await ctx1.newPage();
    const engineerPage = await ctx2.newPage();

    await memberPage.goto("/");
    const membersResp = await memberPage.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0];
    await loginAs(memberPage, member.id);
    await memberPage.goto(`/${member.id}`);
    await waitForMixer(memberPage);

    await engineerPage.goto("/");
    await loginAs(engineerPage, "engineer", "1177");
    await engineerPage.goto("/engineer");
    await waitForMixer(engineerPage);

    await memberPage.waitForTimeout(1000);
    await engineerPage.waitForTimeout(1000);

    // Member clicks SOS
    const alertBtn = memberPage.locator(".alert-btn");
    await expect(alertBtn).toBeVisible({ timeout: 5000 });
    await alertBtn.click({ force: true });

    // Engineer sees toast
    const toast = engineerPage.locator(".alert-toast");
    await expect(toast).toBeVisible({ timeout: 5000 });

    // Wait 6 seconds — toast must STILL be visible (no auto-dismiss)
    await engineerPage.waitForTimeout(6000);
    await expect(toast).toBeVisible();

    // Engineer dismisses
    const dismissBtn = engineerPage.locator(".alert-toast-dismiss");
    await dismissBtn.click({ force: true });

    // Toast disappears
    await expect(toast).not.toBeVisible({ timeout: 3000 });

    // Member button returns to idle (not active)
    await memberPage.waitForTimeout(1000);
    const memberBtnClass = await alertBtn.getAttribute("class");
    expect(memberBtnClass).not.toContain("active");

    await ctx1.close();
    await ctx2.close();
  });

  test("engineer vibration uses pattern array, not single pulses (#162)", async ({ browser }) => {
    const consoleMessages: string[] = [];
    const ctx1 = await browser.newContext();
    const ctx2 = await browser.newContext();
    const memberPage = await ctx1.newPage();
    const engineerPage = await ctx2.newPage();

    engineerPage.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (!text.includes("navigator.vibrate")) {
          consoleMessages.push(`[${msg.type()}] ${text}`);
        }
      }
    });

    // Monkey-patch navigator.vibrate on engineer page to capture calls
    await engineerPage.goto("/");
    await engineerPage.evaluate(() => {
      (window as any).__vibrateCalls = [];
      navigator.vibrate = (pattern: any) => {
        (window as any).__vibrateCalls.push(
          Array.isArray(pattern) ? { type: "pattern", length: pattern.length }
                                 : { type: "single", value: pattern }
        );
        return true;
      };
    });

    await loginAs(engineerPage, "engineer", "1177");
    await engineerPage.goto("/engineer");
    await waitForMixer(engineerPage);

    // Member triggers SOS
    await memberPage.goto("/");
    const membersResp = await memberPage.request.get("/api/members");
    const members = await membersResp.json();
    const member = members[0];
    await loginAs(memberPage, member.id);
    await memberPage.goto(`/${member.id}`);
    await waitForMixer(memberPage);

    // Wait for WS to be ready
    await memberPage.waitForFunction(
      () => document.querySelectorAll(".channel").length > 0,
      { timeout: 10000 },
    );

    // Clear any residual alert
    const alertBtn = memberPage.locator(".alert-btn");
    await expect(alertBtn).toBeVisible({ timeout: 5000 });
    let hadResidual = false;
    try {
      await expect(alertBtn).toHaveClass(/active/, { timeout: 1000 });
      hadResidual = true;
    } catch { /* no residual */ }
    if (hadResidual) {
      await alertBtn.click({ force: true });
      await expect(alertBtn).not.toHaveClass(/active/, { timeout: 5000 });
    }

    // Re-apply monkey-patch (navigation may have cleared it)
    await engineerPage.evaluate(() => {
      (window as any).__vibrateCalls = [];
      navigator.vibrate = (pattern: any) => {
        (window as any).__vibrateCalls.push(
          Array.isArray(pattern) ? { type: "pattern", length: pattern.length }
                                 : { type: "single", value: pattern }
        );
        return true;
      };
    });

    // Member clicks SOS
    await alertBtn.click({ force: true });

    // Engineer should see the toast
    const toast = engineerPage.locator(".alert-toast");
    await expect(toast).toBeVisible({ timeout: 10000 });

    // Wait a moment for vibration to fire
    await engineerPage.waitForTimeout(500);

    // Check vibration calls — should have at least one pattern call, no single 500ms pulses
    const calls = await engineerPage.evaluate(() => (window as any).__vibrateCalls);
    const patternCalls = calls.filter((c: any) => c.type === "pattern");
    const singlePulses = calls.filter((c: any) => c.type === "single" && c.value === 500);

    expect(patternCalls.length).toBeGreaterThanOrEqual(1);
    // Pattern should be [500, 1000] × 30 = 60 elements
    expect(patternCalls[0].length).toBe(60);
    // No old-style single 500ms pulses (vibrate(0) for cancel is OK)
    expect(singlePulses.length).toBe(0);

    // Clean up — dismiss the alert
    const dismissBtn = engineerPage.locator(".alert-toast-dismiss");
    await dismissBtn.click({ force: true });
    await expect(toast).not.toBeVisible({ timeout: 3000 });

    // Zero console errors
    expect(consoleMessages).toEqual([]);

    await ctx1.close();
    await ctx2.close();
  });
});
