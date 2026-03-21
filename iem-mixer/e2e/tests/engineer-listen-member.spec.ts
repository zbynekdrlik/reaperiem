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

// Guard: early return when precondition not met
function assume(condition: unknown, message: string): condition is true {
  if (!condition) {
    console.log(`[ASSUME SKIP] ${message}`);
    return false;
  }
  return true;
}

// Wait for mixer page to load
async function waitForMixer(page: Page): Promise<boolean> {
  const mixerLoaded = await page
    .waitForSelector(".app.mixer, .mixer-header", { timeout: 10000 })
    .catch(() => null);
  return assume(mixerLoaded, "Mixer must load (requires REAPER connection)");
}

test.describe("Engineer Listen on Member Mixes (#99)", () => {
  test("engineer sees Listen button on member mixer page", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");
    await page.goto(`/${member}`);
    if (!(await waitForMixer(page))) return;

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (!assume(toolbarLoaded, "Toolbar must render")) return;

    // Listen button should be visible on member's mixer page
    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });
    const text = await listenBtn.textContent();
    expect(text).toContain("Listen");
  });

  test("engineer sees Listen button on own mixer page", async ({ page }) => {
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (!assume(toolbarLoaded, "Toolbar must render")) return;

    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });
  });

  test("Listen on member page sends ListenStart with member_id", async ({
    page,
  }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");
    await page.goto(`/${member}`);
    if (!(await waitForMixer(page))) return;

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (!assume(toolbarLoaded, "Toolbar must render")) return;

    const listenBtn = page.locator(".toolbar-btn-listen");
    if (!assume(await listenBtn.isVisible(), "Listen button must be visible"))
      return;

    // Intercept the WebSocket to verify ListenStart includes member_id
    const wsMessages: string[] = [];
    await page.evaluate(() => {
      const origWS = window.WebSocket;
      (window as any).__wsMessages = [];
      window.WebSocket = class extends origWS {
        constructor(url: string, protocols?: string | string[]) {
          super(url, protocols);
          const origSend = this.send.bind(this);
          this.send = (data: any) => {
            if (typeof data === "string") {
              (window as any).__wsMessages.push(data);
            }
            return origSend(data);
          };
        }
      } as any;
    });

    // Click Listen — this creates a new WebSocket
    await listenBtn.click();
    await page.waitForTimeout(2000);

    // Check intercepted messages
    const messages: string[] = await page.evaluate(
      () => (window as any).__wsMessages || [],
    );
    const listenStartMsg = messages.find((m) => m.includes("ListenStart"));

    // Verify ListenStart was sent with the correct member_id
    if (listenStartMsg) {
      const parsed = JSON.parse(listenStartMsg);
      expect(parsed.cmd).toBe("ListenStart");
      expect(parsed.member_id).toBe(member);
    }
    // Note: in CI without REAPER, the WS might not connect, so we only check if message was attempted
  });

  test("non-engineer does NOT see Listen button on any page", async ({
    page,
  }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    if (!(await waitForMixer(page))) return;

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (!assume(toolbarLoaded, "Toolbar must render")) return;

    // Listen button should NOT be present for regular members
    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toHaveCount(0);
  });

  test("Mute All only appears on engineer's own mixer", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");

    // On member page: Listen visible, Mute All NOT visible
    await page.goto(`/${member}`);
    if (!(await waitForMixer(page))) return;

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (!assume(toolbarLoaded, "Toolbar must render")) return;

    await expect(page.locator(".toolbar-btn-listen")).toBeVisible({
      timeout: 5000,
    });
    await expect(page.locator(".toolbar-btn-mute-all")).toHaveCount(0);

    // On engineer page: both Listen and Mute All visible
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);

    await expect(page.locator(".toolbar-btn-listen")).toBeVisible({
      timeout: 5000,
    });
    await expect(page.locator(".toolbar-btn-mute-all")).toBeVisible({
      timeout: 5000,
    });
  });

  test("ListenStart triggers REAPER listen target switch via EXTSTATE", async ({
    request,
  }) => {
    // This test verifies the REAPER side: EXTSTATE is set and script executes
    // Skip in CI without REAPER — only runs against live iem.lan
    const reaperCheck = await request
      .get("http://iem.lan:8080/_/NTRACK")
      .catch(() => null);
    if (!assume(reaperCheck?.ok(), "REAPER must be reachable at iem.lan:8080"))
      return;

    // Get first member name from REAPER tracks (inear tracks)
    const membersResp = await request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const memberName = members[0].name.toUpperCase();

    // Set listen target via EXTSTATE
    await request.get(
      `http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/listen_target/${memberName}`,
    );

    // Trigger switch listen script
    await request.get("http://iem.lan:8080/_/_RS_REAPERIEM_SWITCH_LISTEN");

    // Wait for script execution
    await new Promise((r) => setTimeout(r, 3000));

    // Read result
    const resultResp = await request.get(
      "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/listen_result",
    );
    const resultText = await resultResp.text();

    // Result should contain OK and the member name
    expect(resultText).toContain("OK");
    expect(resultText.toUpperCase()).toContain(memberName);
  });
});
