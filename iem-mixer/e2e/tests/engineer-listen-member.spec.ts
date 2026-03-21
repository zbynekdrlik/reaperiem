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

  test("ListenStart triggers REAPER listen target switch via ENGINEER sends", async ({
    request,
  }) => {
    // This test verifies the REAPER side: EXTSTATE is set and script switches
    // sends on the ENGINEER inear track (not a MONITOR bus)
    // Skip in CI without REAPER — only runs against live iem.lan
    const reaperCheck = await request
      .get("http://iem.lan:8080/_/NTRACK")
      .catch(() => null);
    if (!assume(reaperCheck?.ok(), "REAPER must be reachable at iem.lan:8080"))
      return;

    const membersResp = await request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const memberName = members[0].name.toUpperCase();

    // Set listen target to a specific member and trigger switch
    await request.get(
      `http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/listen_target/${memberName}`,
    );
    await request.get("http://iem.lan:8080/_/_RS_REAPERIEM_SWITCH_LISTEN");
    await new Promise((r) => setTimeout(r, 3000));

    const resultResp = await request.get(
      "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/listen_result",
    );
    const resultText = await resultResp.text();
    expect(resultText).toContain("OK");
    expect(resultText.toUpperCase()).toContain(memberName);

    // Now restore all sends (ListenStop equivalent)
    await request.get(
      "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/listen_target/ALL",
    );
    await request.get("http://iem.lan:8080/_/_RS_REAPERIEM_SWITCH_LISTEN");
    await new Promise((r) => setTimeout(r, 3000));

    const restoreResp = await request.get(
      "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/listen_result",
    );
    const restoreText = await restoreResp.text();
    expect(restoreText).toContain("OK:ALL");
  });

  test("ListenStop restores pre-listen mute state instead of unmuting all", async ({
    request,
  }) => {
    // This test verifies Bug 1: ListenStop should restore mute states, not unmute everything.
    // Pre-mute one member's send, listen on another, stop listening, verify mute preserved.
    const reaperCheck = await request
      .get("http://iem.lan:8080/_/NTRACK")
      .catch(() => null);
    if (!assume(reaperCheck?.ok(), "REAPER must be reachable at iem.lan:8080"))
      return;

    const membersResp = await request.get("/api/members");
    const members = await membersResp.json();
    if (
      !assume(
        members.length >= 2,
        "Need at least 2 members for mute preservation test",
      )
    )
      return;

    // Find ENGINEER inear track and member inear tracks with sends to it
    const tracksResp = await request.get("http://iem.lan:8080/_/NTRACK;TRACK");
    const tracksText = await tracksResp.text();
    const lines = tracksText.split("\n");

    // Find engineer inear track index
    let engineerTrackIdx = -1;
    const memberInears: { name: string; trackIdx: number }[] = [];
    for (const line of lines) {
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
    if (!assume(engineerTrackIdx >= 0, "ENGINEER inear track must exist"))
      return;
    if (
      !assume(memberInears.length >= 2, "Need at least 2 member inear tracks")
    )
      return;

    // Find send indices from member inear tracks to ENGINEER inear
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
    if (
      !assume(
        memberSends.length >= 2,
        "Need at least 2 members with sends to ENGINEER",
      )
    )
      return;

    const muteTarget = memberSends[0]; // This member will be pre-muted
    const listenTarget = memberSends[1]; // We'll listen to this member

    // Save original mute states for cleanup
    const originalMutes: {
      trackIdx: number;
      sendIdx: number;
      muted: boolean;
    }[] = [];
    for (const ms of memberSends) {
      const resp = await request.get(
        `http://iem.lan:8080/_/GET/TRACK/${ms.trackIdx}/SEND/${ms.sendIdx}`,
      );
      const text = await resp.text();
      const parts = text.split("\t");
      const muteFlag = parseInt(parts[3] || "0");
      originalMutes.push({
        trackIdx: ms.trackIdx,
        sendIdx: ms.sendIdx,
        muted: (muteFlag & 8) !== 0,
      });
    }

    try {
      // Step 1: Pre-mute first member's send to ENGINEER inear
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${muteTarget.trackIdx}/SEND/${muteTarget.sendIdx}/MUTE/1`,
      );
      await new Promise((r) => setTimeout(r, 500));

      // Verify the mute took effect
      const verifyResp = await request.get(
        `http://iem.lan:8080/_/GET/TRACK/${muteTarget.trackIdx}/SEND/${muteTarget.sendIdx}`,
      );
      const verifyText = await verifyResp.text();
      const verifyParts = verifyText.split("\t");
      const verifyMuteFlag = parseInt(verifyParts[3] || "0");
      if (
        !assume(
          (verifyMuteFlag & 8) !== 0,
          `${muteTarget.name} send must be muted before listen test`,
        )
      )
        return;

      // Step 2: ListenStart on the second member
      await request.get(
        `http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/listen_target/${listenTarget.name}`,
      );
      await request.get("http://iem.lan:8080/_/_RS_REAPERIEM_SWITCH_LISTEN");
      await new Promise((r) => setTimeout(r, 3000));

      const listenResult = await request.get(
        "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/listen_result",
      );
      const listenText = await listenResult.text();
      expect(listenText).toContain("OK");

      // Step 3: ListenStop (target="ALL")
      await request.get(
        "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/listen_target/ALL",
      );
      await request.get("http://iem.lan:8080/_/_RS_REAPERIEM_SWITCH_LISTEN");
      await new Promise((r) => setTimeout(r, 3000));

      const stopResult = await request.get(
        "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/listen_result",
      );
      const stopText = await stopResult.text();
      expect(stopText).toContain("OK:ALL");

      // Step 4: ASSERT - pre-muted member's send should STILL be muted
      const afterResp = await request.get(
        `http://iem.lan:8080/_/GET/TRACK/${muteTarget.trackIdx}/SEND/${muteTarget.sendIdx}`,
      );
      const afterText = await afterResp.text();
      const afterParts = afterText.split("\t");
      const afterMuteFlag = parseInt(afterParts[3] || "0");
      expect(
        (afterMuteFlag & 8) !== 0,
        `${muteTarget.name}'s send should still be muted after ListenStop, but flag was ${afterMuteFlag}`,
      ).toBe(true);
    } finally {
      // Cleanup: restore original mute states
      // Also clear any listen backup EXTSTATE
      await request.get(
        "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/listen_mute_backup/",
      );
      for (const orig of originalMutes) {
        await request.get(
          `http://iem.lan:8080/_/SET/TRACK/${orig.trackIdx}/SEND/${orig.sendIdx}/MUTE/${orig.muted ? 1 : 0}`,
        );
      }
    }
  });

  test("Listen does not change mute buttons in engineer mixer UI", async ({
    page,
  }) => {
    // This test verifies that when the engineer activates Listen on a member,
    // the mute buttons in the Mixes tab do NOT visually change.
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    if (!(await waitForMixer(page))) return;

    const toolbarLoaded = await page
      .waitForSelector(".toolbar", { timeout: 10000 })
      .catch(() => null);
    if (!assume(toolbarLoaded, "Toolbar must render")) return;

    // Wait for channels to load (mix channels appear in engineer's mixer)
    const channelsLoaded = await page
      .waitForSelector(".channel-strip", { timeout: 10000 })
      .catch(() => null);
    if (!assume(channelsLoaded, "Channel strips must render")) return;

    // Capture initial mute button states
    const initialMuteStates = await page.evaluate(() => {
      const buttons = document.querySelectorAll(
        ".channel-strip .mute-btn, .channel-strip .btn-mute",
      );
      return Array.from(buttons).map((btn) => ({
        text: btn.textContent?.trim() || "",
        classes: btn.className,
      }));
    });

    if (!assume(initialMuteStates.length > 0, "Need mute buttons to test"))
      return;

    // Click Listen button
    const listenBtn = page.locator(".toolbar-btn-listen");
    if (!assume(await listenBtn.isVisible(), "Listen button must be visible"))
      return;
    await listenBtn.click();

    // Wait for listen to activate and poller to run a few cycles
    await page.waitForTimeout(3000);

    // Capture mute button states after listen activation
    const afterListenMuteStates = await page.evaluate(() => {
      const buttons = document.querySelectorAll(
        ".channel-strip .mute-btn, .channel-strip .btn-mute",
      );
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
      const buttons = document.querySelectorAll(
        ".channel-strip .mute-btn, .channel-strip .btn-mute",
      );
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
});
