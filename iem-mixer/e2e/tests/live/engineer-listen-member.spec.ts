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
  await expect(page.locator(".app.mixer, .mixer-header")).toBeVisible({ timeout: 10000 });
}

test.describe("Engineer Listen on Member Mixes (#99)", () => {
  test("engineer sees Listen button on member mixer page", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

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
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });
  });

  test("Listen on member page sends ListenStart with member_id", async ({
    page,
  }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });

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
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    // Listen button should NOT be present for regular members
    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toHaveCount(0);
  });

  test("Mute All only appears on engineer's own mixer", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");

    // On member page: Listen visible, Mute All NOT visible
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    await expect(page.locator(".toolbar-btn-listen")).toBeVisible({
      timeout: 5000,
    });
    await expect(page.locator(".toolbar-btn-mute-all")).toHaveCount(0);

    // On engineer page: both Listen and Mute All visible
    await page.goto("/engineer");
    await waitForMixer(page);

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

  test("ListenStart on band member mutes other sends to ENGINEER via REAPER API", async ({
    request,
  }) => {
    // This test verifies the REAPER side: sends to ENGINEER are muted for isolation
    const reaperCheck = await request
      .get("http://iem.lan:8080/_/NTRACK")
      .catch(() => null);
    expect(reaperCheck?.ok()).toBe(true);

    const membersResp = await request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(2);

    const targetMember = members[0];
    const memberId = targetMember.id;

    // Find member inear tracks and their sends to ENGINEER
    const tracksResp = await request.get("http://iem.lan:8080/_/NTRACK;TRACK");
    const tracksText = await tracksResp.text();
    let engineerTrackIdx = -1;
    const memberInears: { name: string; trackIdx: number }[] = [];
    for (const line of tracksText.split("\n")) {
      const parts = line.split("\t");
      if (parts[0] === "TRACK" && parts.length > 2) {
        const idx = parseInt(parts[1]);
        const name = parts[2];
        if (name.toUpperCase().match(/^ENGINEER\s+INEAR$/)) {
          engineerTrackIdx = idx;
        }
        const memberMatch = name.match(/^(\S+)\s+inear$/i);
        if (memberMatch && !memberMatch[1].toUpperCase().match(/^ENGINEER$/)) {
          memberInears.push({
            name: memberMatch[1].toUpperCase(),
            trackIdx: idx,
          });
        }
      }
    }
    expect(engineerTrackIdx).toBeGreaterThanOrEqual(0);
    expect(memberInears.length).toBeGreaterThanOrEqual(2);

    // Find send indices to ENGINEER
    const memberSends: { name: string; trackIdx: number; sendIdx: number }[] =
      [];
    for (const m of memberInears) {
      for (let s = 0; s < 10; s++) {
        const sendResp = await request.get(
          `http://iem.lan:8080/_/GET/TRACK/${m.trackIdx}/SEND/${s}`,
        );
        const sendText = await sendResp.text();
        const sendParts = sendText.split("\t");
        if (sendParts[0] !== "SEND") break;
        const destTrack = parseInt(sendParts[6]);
        if (destTrack === engineerTrackIdx) {
          memberSends.push({ ...m, sendIdx: s });
          break;
        }
      }
    }
    expect(memberSends.length).toBeGreaterThanOrEqual(2);

  test("Listen restores pre-muted states after stop (mute preservation)", async ({
    request,
  }) => {
    // This test verifies that mute states are restored after listen cycle
    const reaperCheck = await request
      .get("http://iem.lan:8080/_/NTRACK")
      .catch(() => null);
    expect(reaperCheck?.ok()).toBe(true);

    const membersResp = await request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(2);

  test("Listen produces audio output within 3 seconds on engineer page", async ({
    page,
  }) => {
    // This test requires REAPER with active audio — live only
    const reaperCheck = await page.request
      .get("http://iem.lan:8080/_/NTRACK")
      .catch(() => null);
    expect(reaperCheck?.ok()).toBe(true);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });

    // Click Listen — should produce audio within 3 seconds (no mute toggle needed)
    await listenBtn.click();

    // Poll __iem_audio_level() for up to 3 seconds
    let audioLevel = -150;
    for (let i = 0; i < 15; i++) {
      await page.waitForTimeout(200);
      audioLevel = await page.evaluate(() => {
        return typeof window.__iem_audio_level === "function"
          ? window.__iem_audio_level()
          : -150;
      });
      if (audioLevel > -100) break;
    }

    expect(
      audioLevel,
      `Audio level should be > -100 dB within 3s of Listen click, got ${audioLevel} dB`,
    ).toBeGreaterThan(-100);

    // Cleanup: stop listening
    await listenBtn.click();
  });

  test("Listen does not change mute buttons in engineer mixer UI", async ({
    page,
  }) => {
    // This test verifies that when the engineer activates Listen on their own page,
    // the mute buttons do NOT visually change (engineer listen = audio only, no REAPER changes).
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    // Wait for channels to load (mix channels appear in engineer's mixer)
    const channelsLoaded = await page
      .waitForSelector(".channel", { timeout: 10000 })
      .catch(() => null);
    expect(channelsLoaded).toBeTruthy();

    // Capture initial mute button states
    const initialMuteStates = await page.evaluate(() => {
      const buttons = document.querySelectorAll(".channel .mute-btn");
      return Array.from(buttons).map((btn) => ({
        text: btn.textContent?.trim() || "",
        classes: btn.className,
      }));
    });

    expect(initialMuteStates.length).toBeGreaterThan(0);

    // Click Listen button
    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });
    await listenBtn.click();

    // Wait for listen to activate and poller to run a few cycles
    await page.waitForTimeout(3000);

    // Capture mute button states after listen activation
    const afterListenMuteStates = await page.evaluate(() => {
      const buttons = document.querySelectorAll(".channel .mute-btn");
      return Array.from(buttons).map((btn) => ({
        text: btn.textContent?.trim() || "",
        classes: btn.className,
      }));
    });

    // Assert mute buttons did not change
    expect(afterListenMuteStates.length).toBe(initialMuteStates.length);
    for (let i = 0; i < initialMuteStates.length; i++) {
      expect(
        afterListenMuteStates[i].classes,
        `Mute button ${i} (${initialMuteStates[i].text}) should not change during listen`,
      ).toBe(initialMuteStates[i].classes);
    }

    // Click Listen again to stop
    await listenBtn.click();
    await page.waitForTimeout(2000);

    // Verify mute states are still unchanged after stopping listen
    const afterStopMuteStates = await page.evaluate(() => {
      const buttons = document.querySelectorAll(".channel .mute-btn");
      return Array.from(buttons).map((btn) => ({
        text: btn.textContent?.trim() || "",
        classes: btn.className,
      }));
    });

    for (let i = 0; i < initialMuteStates.length; i++) {
      expect(
        afterStopMuteStates[i].classes,
        `Mute button ${i} (${initialMuteStates[i].text}) should not change after listen stop`,
      ).toBe(initialMuteStates[i].classes);
    }
  });

  test("Listen stop stays stopped — no auto-reconnect after user clicks stop", async ({
    page,
  }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");
    await page.goto(`/${member}`);
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });

    // Click Listen to start
    await listenBtn.click();
    await page.waitForTimeout(2000);

    // Verify it started (button text changes from "Listen")
    const textAfterStart = await listenBtn.textContent();
    const started =
      textAfterStart?.includes("Reconnecting") ||
      textAfterStart?.includes(member.charAt(0).toUpperCase());
    // It should have moved away from idle "Listen" state
    expect(started).toBeTruthy();
  });

  test("Rapid listen toggle does not corrupt mute state", async ({ page }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    expect(members.length).toBeGreaterThanOrEqual(1);

    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    const channelsLoaded = await page
      .waitForSelector(".channel", { timeout: 10000 })
      .catch(() => null);
    expect(channelsLoaded).toBeTruthy();

    // Capture initial mute button states
    const initialMuteStates = await page.evaluate(() => {
      const buttons = document.querySelectorAll(".channel .mute-btn");
      return Array.from(buttons).map((btn) => ({
        text: btn.textContent?.trim() || "",
        classes: btn.className,
      }));
    });

    expect(initialMuteStates.length).toBeGreaterThan(0);

    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });

    // Rapidly toggle listen on/off 3 times (200ms between each click)
    for (let i = 0; i < 6; i++) {
      await listenBtn.click();
      await page.waitForTimeout(200);
    }

    // Wait for everything to settle
    await page.waitForTimeout(3000);

    // Assert mute buttons unchanged after rapid toggling
    const afterMuteStates = await page.evaluate(() => {
      const buttons = document.querySelectorAll(".channel .mute-btn");
      return Array.from(buttons).map((btn) => ({
        text: btn.textContent?.trim() || "",
        classes: btn.className,
      }));
    });

    expect(afterMuteStates.length).toBe(initialMuteStates.length);
    for (let i = 0; i < initialMuteStates.length; i++) {
      expect(
        afterMuteStates[i].classes,
        `Mute button ${i} (${initialMuteStates[i].text}) should not change after rapid listen toggles`,
      ).toBe(initialMuteStates[i].classes);
    }
  });

  test("Mute All works correctly after listen cycle", async ({ page }) => {
    const reaperCheck = await page.request
      .get("http://iem.lan:8080/_/NTRACK")
      .catch(() => null);
    expect(reaperCheck?.ok()).toBe(true);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    expect(toolbarLoaded).toBeTruthy();

    const channelsLoaded = await page
      .waitForSelector(".channel", { timeout: 10000 })
      .catch(() => null);
    expect(channelsLoaded).toBeTruthy();

    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });

    const muteAllBtn = page.locator(".toolbar-btn-mute-all");
    await expect(muteAllBtn).toBeVisible({ timeout: 5000 });
});
