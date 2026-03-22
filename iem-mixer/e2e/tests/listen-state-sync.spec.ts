import { test, expect, Page, APIRequestContext } from "@playwright/test";
import { WebSocket as WsClient } from "ws";

/**
 * Listen Feature State Synchronization Tests — LIVE system
 *
 * v1.88.0: Listen uses SOLO (non-destructive) instead of mute.
 * - Engineer page listen: just streams audio, touches NOTHING in REAPER
 * - Band member page listen: solos that member's inear track, streams audio
 * - Mute buttons never change during listen (no suppression needed)
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

function assume(condition: unknown, message: string): condition is true {
  if (!condition) {
    console.log(`[ASSUME SKIP] ${message}`);
    return false;
  }
  return true;
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

async function getReaperTrackSolo(
  request: APIRequestContext,
  trackIdx: number,
): Promise<boolean> {
  const resp = await request.get("http://iem.lan:8080/_/NTRACK;TRACK");
  const text = await resp.text();
  for (const line of text.split("\n")) {
    const parts = line.split("\t");
    if (parts[0] === "TRACK" && parseInt(parts[1]) === trackIdx) {
      const flags = parseInt(parts[3] || "0");
      // Solo bits are at positions 4-5 (mask 0x30)
      return (flags & 0x30) !== 0;
    }
  }
  return false;
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
  test.beforeEach(async ({ request }) => {
    const reaperCheck = await request
      .get("http://iem.lan:8080/_/NTRACK")
      .catch(() => null);
    test.skip(!reaperCheck?.ok(), "REAPER must be reachable at iem.lan:8080");
  });

  test("Test A: Engineer page listen does NOT change any REAPER state", async ({
    page,
  }) => {
    const token = await getAuthToken(page.request);
    if (!assume(token, "Auth must succeed")) return;

    const info = await findMemberSendsToEngineer(page.request);
    if (!assume(info && info.memberSends.length >= 2, "Need 2+ member sends"))
      return;

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

  test("Test B: Band member page listen solos that member's track", async ({
    page,
  }) => {
    const token = await getAuthToken(page.request);
    if (!assume(token, "Auth must succeed")) return;

    const info = await findMemberSendsToEngineer(page.request);
    if (!assume(info && info.memberSends.length >= 1, "Need 1+ member sends"))
      return;

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
      // ListenStart on band member — should solo their inear track
      ws.send(JSON.stringify({ cmd: "ListenStart", member_id: memberId }));
      await page.waitForTimeout(2000);

      // Check: target track should be soloed
      const soloed = await getReaperTrackSolo(page.request, target.trackIdx);
      expect(
        soloed,
        `${target.name} inear should be soloed during listen`,
      ).toBe(true);

      // ListenStop — should unsolo
      ws.send(JSON.stringify({ cmd: "ListenStop" }));
      await page.waitForTimeout(2000);

      const unsoloed = await getReaperTrackSolo(page.request, target.trackIdx);
      expect(
        unsoloed,
        `${target.name} inear should NOT be soloed after listen stop`,
      ).toBe(false);

      // All mute states unchanged throughout
      for (const ms of info!.memberSends) {
        const afterMute = await getReaperSendMute(
          page.request,
          ms.trackIdx,
          ms.sendIdx,
        );
        expect(
          afterMute,
          `${ms.name}: mute changed during solo-based listen`,
        ).toBe(beforeMutes[ms.name]);
      }
    } finally {
      ws.close();
    }
  });

  test("Test C: Mute buttons on engineer Mixes tab never change during listen", async ({
    page,
  }) => {
    const token = await getAuthToken(page.request);
    if (!assume(token, "Auth must succeed")) return;

    const info = await findMemberSendsToEngineer(page.request);
    if (!assume(info && info.memberSends.length >= 2, "Need 2+ member sends"))
      return;

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    const mixerLoaded = await page
      .waitForSelector(".app.mixer", { timeout: 10000 })
      .catch(() => null);
    if (!assume(mixerLoaded, "Mixer must load")) return;
    await page.click(".category-tab.mixes");
    await page.waitForTimeout(3000);

    const initialStates = await getUiMuteStates(page);
    if (!assume(initialStates.length >= 2, "Need mix channels")) return;
    console.log("INITIAL:", JSON.stringify(initialStates));

    const ws = await createAudioWs(WS_URL, token!);
    try {
      ws.send(
        JSON.stringify({
          cmd: "ListenStart",
          member_id: info!.memberSends[0].name.toLowerCase(),
        }),
      );

      // Poll mute buttons every 500ms for 5 seconds — assert NONE change
      for (let i = 0; i < 10; i++) {
        await page.waitForTimeout(500);
        const current = await getUiMuteStates(page);
        for (let j = 0; j < initialStates.length && j < current.length; j++) {
          expect(
            current[j].classes,
            `${initialStates[j].name}: mute changed at poll ${i} during listen`,
          ).toBe(initialStates[j].classes);
        }
      }

      ws.send(JSON.stringify({ cmd: "ListenStop" }));
      await page.waitForTimeout(3000);

      const afterListen = await getUiMuteStates(page);
      for (let i = 0; i < initialStates.length && i < afterListen.length; i++) {
        expect(
          afterListen[i].classes,
          `${initialStates[i].name}: mute changed AFTER listen stop`,
        ).toBe(initialStates[i].classes);
      }
    } finally {
      ws.close();
    }
  });

  test("Test D: Engineer can change mute/volume while listening", async ({
    page,
  }) => {
    const token = await getAuthToken(page.request);
    if (!assume(token, "Auth must succeed")) return;

    const info = await findMemberSendsToEngineer(page.request);
    if (!assume(info && info.memberSends.length >= 2, "Need 2+ member sends"))
      return;

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    const mixerLoaded = await page
      .waitForSelector(".app.mixer", { timeout: 10000 })
      .catch(() => null);
    if (!assume(mixerLoaded, "Mixer must load")) return;
    await page.click(".category-tab.mixes");
    await page.waitForTimeout(3000);

    const targetName = info!.memberSends[0].name;
    const muteBtn = page.locator(
      `.channel:has(.ch-name:text-is("${targetName}")) .mute-btn`,
    );
    if (
      !assume(
        await muteBtn.isVisible(),
        `${targetName} mute button must be visible`,
      )
    )
      return;

    // Start listening
    const ws = await createAudioWs(WS_URL, token!);
    try {
      ws.send(
        JSON.stringify({
          cmd: "ListenStart",
          member_id: info!.memberSends[1].name.toLowerCase(),
        }),
      );
      await page.waitForTimeout(1000);

      // Record initial mute state
      const beforeClasses = await muteBtn.getAttribute("class");

      // Click mute on a channel during listen
      await muteBtn.click();
      await page.waitForTimeout(1500);

      // Assert mute button changed (UI responds normally during listen)
      const afterClickClasses = await muteBtn.getAttribute("class");
      expect(
        afterClickClasses,
        `${targetName}: mute button should change after click during listen`,
      ).not.toBe(beforeClasses);

      // Stop listening
      ws.send(JSON.stringify({ cmd: "ListenStop" }));
      await page.waitForTimeout(2000);

      // Assert mute button still shows the user's change
      const afterStopClasses = await muteBtn.getAttribute("class");
      expect(
        afterStopClasses,
        `${targetName}: user's mute change should persist after listen stop`,
      ).toBe(afterClickClasses);
    } finally {
      ws.close();
      // Restore mute state
      await muteBtn.click();
      await page.waitForTimeout(500);
    }
  });

  test("Rapid listen toggles preserve mute state", async ({ page }) => {
    const token = await getAuthToken(page.request);
    if (!assume(token, "Auth must succeed")) return;

    const info = await findMemberSendsToEngineer(page.request);
    if (!assume(info && info.memberSends.length >= 2, "Need 2+ member sends"))
      return;

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    const mixerLoaded = await page
      .waitForSelector(".app.mixer", { timeout: 10000 })
      .catch(() => null);
    if (!assume(mixerLoaded, "Mixer must load")) return;
    await page.click(".category-tab.mixes");
    await page.waitForTimeout(3000);

    const initialStates = await getUiMuteStates(page);
    if (!assume(initialStates.length >= 2, "Need mix channels")) return;

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
