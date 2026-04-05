import { test, expect, Page, APIRequestContext } from "@playwright/test";
import { WebSocket as WsClient } from "ws";

/**
 * Listen Feature State Synchronization Tests — LIVE system
 *
 * v1.89.0: Listen uses send muting for band member audio isolation.
 * - Engineer page listen: just streams audio, touches NOTHING in REAPER
 * - Band member page listen: mutes all other member sends to ENGINEER except target
 * - On stop, all mute states are restored to their original values
 */

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


async function getAuthToken(
  request: APIRequestContext,
): Promise<string | null> {
  const resp = await request.post("/api/auth", {
    data: { member: "engineer", pin: "1177" },
  });
  if (resp.status() !== 200) return null;
  const data = await resp.json();
  return data.token;
}

interface MemberSend {
  name: string;
  trackIdx: number;
  sendIdx: number;
}

async function findMemberSendsToEngineer(
  request: APIRequestContext,
): Promise<{ engineerTrackIdx: number; memberSends: MemberSend[] } | null> {
  const tracksResp = await request.get("http://iem.lan:8080/_/NTRACK;TRACK");
  const tracksText = await tracksResp.text();
  const lines = tracksText.split("\n");

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
  if (engineerTrackIdx < 0 || memberInears.length < 2) return null;

  const memberSends: MemberSend[] = [];
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
  return { engineerTrackIdx, memberSends };
}

async function getReaperSendMute(
  request: APIRequestContext,
  trackIdx: number,
  sendIdx: number,
): Promise<boolean> {
  const resp = await request.get(
    `http://iem.lan:8080/_/GET/TRACK/${trackIdx}/SEND/${sendIdx}`,
  );
  const text = await resp.text();
  const parts = text.split("\t");
  const muteFlag = parseInt(parts[3] || "0");
  return (muteFlag & 8) !== 0;
}

function createAudioWs(wsUrl: string, token: string): Promise<WsClient> {
  return new Promise((resolve, reject) => {
    const ws = new WsClient(`${wsUrl}?token=${token}`);
    const timeout = setTimeout(() => {
      ws.close();
      reject(new Error("WebSocket connect timeout"));
    }, 5000);

    ws.on("open", () => {
      clearTimeout(timeout);
      resolve(ws);
    });

    ws.on("error", (err) => {
      clearTimeout(timeout);
      reject(err);
    });
  });
}

const WS_URL = "ws://10.77.9.231/ws/audio";

async function getUiMuteStates(page: Page) {
  return page.evaluate(() => {
    const channels = document.querySelectorAll(".channel");
    return Array.from(channels)
      .map((ch) => {
        const name = ch.querySelector(".ch-name")?.textContent?.trim() || "";
        const muteBtn = ch.querySelector(".mute-btn");
        return {
          name,
          isMuted: muteBtn?.className.includes(" on") ?? false,
          classes: muteBtn?.className || "",
        };
      })
      .filter((ch) => ch.name !== "");
  });
}

test.describe("Listen State Sync — LIVE system via WebSocket", () => {
  test("Test A: Engineer page listen does NOT change any REAPER state", async ({
    page,
  }) => {
    const reaperCheck = await page.request
      .get("http://iem.lan:8080/_/NTRACK");
    expect(reaperCheck.ok()).toBe(true);

    const token = await getAuthToken(page.request);
    expect(token).toBeTruthy();

    const info = await findMemberSendsToEngineer(page.request);
    expect(info && info.memberSends.length).toBeGreaterThanOrEqual(2);

    // Record all send mute states before listen
    const beforeMutes: Record<string, boolean> = {};
    for (const ms of info!.memberSends) {
      beforeMutes[ms.name] = await getReaperSendMute(
        page.request,
        ms.trackIdx,
        ms.sendIdx,
      );
    }

    const ws = await createAudioWs(WS_URL, token!);
    try {
      // ListenStart with engineer (no solo should happen)
      ws.send(JSON.stringify({ cmd: "ListenStart", member_id: "engineer" }));
      await page.waitForTimeout(5000);

      // Assert ALL send mute states unchanged
      for (const ms of info!.memberSends) {
        const duringMute = await getReaperSendMute(
          page.request,
          ms.trackIdx,
          ms.sendIdx,
        );
        expect(
          duringMute,
          `${ms.name}: mute changed DURING engineer listen`,
        ).toBe(beforeMutes[ms.name]);
      }

      ws.send(JSON.stringify({ cmd: "ListenStop" }));
      await page.waitForTimeout(2000);

      // Assert ALL send mute states still unchanged
      for (const ms of info!.memberSends) {
        const afterMute = await getReaperSendMute(
          page.request,
          ms.trackIdx,
          ms.sendIdx,
        );
        expect(
          afterMute,
          `${ms.name}: mute changed AFTER engineer listen stop`,
        ).toBe(beforeMutes[ms.name]);
      }
    } finally {
      ws.close();
    }
  });

  test("Test B: Band member page listen mutes other sends to ENGINEER", async ({
    page,
  }) => {
    const reaperCheck = await page.request
      .get("http://iem.lan:8080/_/NTRACK");
    expect(reaperCheck.ok()).toBe(true);

    const token = await getAuthToken(page.request);
    expect(token).toBeTruthy();

    const info = await findMemberSendsToEngineer(page.request);
    expect(info && info.memberSends.length).toBeGreaterThanOrEqual(2);

    const target = info!.memberSends[0];
    const memberId = target.name.toLowerCase();

    // Record mute states before
    const beforeMutes: Record<string, boolean> = {};
    for (const ms of info!.memberSends) {
      beforeMutes[ms.name] = await getReaperSendMute(
        page.request,
        ms.trackIdx,
        ms.sendIdx,
      );
    }

    const ws = await createAudioWs(WS_URL, token!);
    try {
      // ListenStart on band member — should mute all other sends, unmute target's
      ws.send(JSON.stringify({ cmd: "ListenStart", member_id: memberId }));
      await page.waitForTimeout(2000);

      // Check: target member's send should be unmuted
      const targetMute = await getReaperSendMute(
        page.request,
        target.trackIdx,
        target.sendIdx,
      );
      expect(
        targetMute,
        `${target.name} send to ENGINEER should be unmuted during listen`,
      ).toBe(false);

      // Check: all other members' sends should be muted
      for (const ms of info!.memberSends) {
        if (ms.name === target.name) continue;
        const muted = await getReaperSendMute(
          page.request,
          ms.trackIdx,
          ms.sendIdx,
        );
        expect(
          muted,
          `${ms.name} send to ENGINEER should be muted during listen`,
        ).toBe(true);
      }

      // ListenStop — should restore original mute states
      ws.send(JSON.stringify({ cmd: "ListenStop" }));
      await page.waitForTimeout(2000);

      // All mute states restored to original
      for (const ms of info!.memberSends) {
        const afterMute = await getReaperSendMute(
          page.request,
          ms.trackIdx,
          ms.sendIdx,
        );
        expect(
          afterMute,
          `${ms.name}: mute not restored after listen stop (was ${beforeMutes[ms.name]}, got ${afterMute})`,
        ).toBe(beforeMutes[ms.name]);
      }
    } finally {
      ws.close();
    }
  });

  test("Test C: Mute buttons restore after band member listen stop", async ({
    page,
  }) => {
    const reaperCheck = await page.request
      .get("http://iem.lan:8080/_/NTRACK");
    expect(reaperCheck.ok()).toBe(true);

    const token = await getAuthToken(page.request);
    expect(token).toBeTruthy();

    const info = await findMemberSendsToEngineer(page.request);
    expect(info && info.memberSends.length).toBeGreaterThanOrEqual(2);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await expect(page.locator(".app.mixer")).toBeVisible({ timeout: 10000 });
    await page.click(".category-tab.mixes");
    await page.waitForTimeout(3000);

    const initialStates = await getUiMuteStates(page);
    expect(initialStates.length).toBeGreaterThanOrEqual(2);
    console.log("INITIAL:", JSON.stringify(initialStates));

    const ws = await createAudioWs(WS_URL, token!);
    try {
      ws.send(
        JSON.stringify({
          cmd: "ListenStart",
          member_id: info!.memberSends[0].name.toLowerCase(),
        }),
      );
      // Wait for mute changes to propagate via poller
      await page.waitForTimeout(5000);

      // After ListenStop, mute buttons should restore to original states
      ws.send(JSON.stringify({ cmd: "ListenStop" }));
      await page.waitForTimeout(5000);

      const afterListen = await getUiMuteStates(page);
      for (let i = 0; i < initialStates.length && i < afterListen.length; i++) {
        expect(
          afterListen[i].classes,
          `${initialStates[i].name}: mute not restored after listen stop`,
        ).toBe(initialStates[i].classes);
      }
    } finally {
      ws.close();
    }
  });

  test("Test D: Mute states restore to pre-listen values after stop", async ({
    page,
  }) => {
    const reaperCheck = await page.request
      .get("http://iem.lan:8080/_/NTRACK");
    expect(reaperCheck.ok()).toBe(true);

    const token = await getAuthToken(page.request);
    expect(token).toBeTruthy();

    const info = await findMemberSendsToEngineer(page.request);
    expect(info && info.memberSends.length).toBeGreaterThanOrEqual(2);

    // Pre-mute first member's send to verify it's preserved through listen cycle
    const muteTarget = info!.memberSends[0];
    await page.request.get(
      `http://iem.lan:8080/_/SET/TRACK/${muteTarget.trackIdx}/SEND/${muteTarget.sendIdx}/MUTE/1`,
    );
    await page.waitForTimeout(500);

    // Record all mute states before listen
    const beforeMutes: Record<string, boolean> = {};
    for (const ms of info!.memberSends) {
      beforeMutes[ms.name] = await getReaperSendMute(
        page.request,
        ms.trackIdx,
        ms.sendIdx,
      );
    }
    expect(
      beforeMutes[muteTarget.name],
      "Pre-muted member should be muted",
    ).toBe(true);

    const ws = await createAudioWs(WS_URL, token!);
    try {
      // Listen on second member
      ws.send(
        JSON.stringify({
          cmd: "ListenStart",
          member_id: info!.memberSends[1].name.toLowerCase(),
        }),
      );
      await page.waitForTimeout(2000);

      // Stop listening — should restore all mute states
      ws.send(JSON.stringify({ cmd: "ListenStop" }));
      await page.waitForTimeout(2000);

      // All mute states should be restored to pre-listen values
      for (const ms of info!.memberSends) {
        const afterMute = await getReaperSendMute(
          page.request,
          ms.trackIdx,
          ms.sendIdx,
        );
        expect(
          afterMute,
          `${ms.name}: mute not restored (expected ${beforeMutes[ms.name]}, got ${afterMute})`,
        ).toBe(beforeMutes[ms.name]);
      }
    } finally {
      ws.close();
      // Restore: unmute all sends for clean state
      for (const ms of info!.memberSends) {
        await page.request.get(
          `http://iem.lan:8080/_/SET/TRACK/${ms.trackIdx}/SEND/${ms.sendIdx}/MUTE/0`,
        );
      }
    }
  });

  test("Rapid listen toggles preserve mute state", async ({ page }) => {
    const reaperCheck = await page.request
      .get("http://iem.lan:8080/_/NTRACK");
    expect(reaperCheck.ok()).toBe(true);

    const token = await getAuthToken(page.request);
    expect(token).toBeTruthy();

    const info = await findMemberSendsToEngineer(page.request);
    expect(info && info.memberSends.length).toBeGreaterThanOrEqual(2);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await expect(page.locator(".app.mixer")).toBeVisible({ timeout: 10000 });
    await page.click(".category-tab.mixes");
    await page.waitForTimeout(3000);

    const initialStates = await getUiMuteStates(page);
    expect(initialStates.length).toBeGreaterThanOrEqual(2);

    const ws = await createAudioWs(WS_URL, token!);
    try {
      const memberId = info!.memberSends[0].name.toLowerCase();
      for (let i = 0; i < 3; i++) {
        ws.send(JSON.stringify({ cmd: "ListenStart", member_id: memberId }));
        await page.waitForTimeout(300);
        ws.send(JSON.stringify({ cmd: "ListenStop" }));
        await page.waitForTimeout(300);
      }

      await page.waitForTimeout(3000);

      const afterRapid = await getUiMuteStates(page);
      for (let i = 0; i < initialStates.length && i < afterRapid.length; i++) {
        expect(
          afterRapid[i].classes,
          `${initialStates[i].name}: mute corrupted by rapid toggle!`,
        ).toBe(initialStates[i].classes);
      }
    } finally {
      ws.close();
    }
  });
});
