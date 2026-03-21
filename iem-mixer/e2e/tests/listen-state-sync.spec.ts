import { test, expect, Page, APIRequestContext } from "@playwright/test";
import { WebSocket as WsClient } from "ws";

/**
 * Listen Feature State Synchronization Tests — LIVE system
 *
 * These tests exercise the REAL WebSocket code path to reproduce
 * mute/listen state desync bugs on the deployed system.
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

  test("Bug 1: Mute buttons stable after listen cycle (persistent WS)", async ({
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
      await page.waitForTimeout(2000);

      const duringListen = await getUiMuteStates(page);
      console.log("DURING LISTEN:", JSON.stringify(duringListen));
      for (
        let i = 0;
        i < initialStates.length && i < duringListen.length;
        i++
      ) {
        expect(
          duringListen[i].classes,
          `${initialStates[i].name}: mute changed DURING listen`,
        ).toBe(initialStates[i].classes);
      }

      ws.send(JSON.stringify({ cmd: "ListenStop" }));
      await page.waitForTimeout(6000);

      const afterListen = await getUiMuteStates(page);
      console.log("AFTER LISTEN:", JSON.stringify(afterListen));

      for (let i = 0; i < initialStates.length && i < afterListen.length; i++) {
        expect(
          afterListen[i].classes,
          `${initialStates[i].name}: mute changed AFTER listen stop — BUG 1!`,
        ).toBe(initialStates[i].classes);
      }
    } finally {
      ws.close();
    }
  });

  test("Bug 2: UI matches REAPER after listen (persistent WS)", async ({
    page,
  }) => {
    const token = await getAuthToken(page.request);
    if (!assume(token, "Auth must succeed")) return;

    const info = await findMemberSendsToEngineer(page.request);
    if (!assume(info && info.memberSends.length >= 2, "Need 2+ member sends"))
      return;
    const { memberSends } = info!;

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    const mixerLoaded = await page
      .waitForSelector(".app.mixer", { timeout: 10000 })
      .catch(() => null);
    if (!assume(mixerLoaded, "Mixer must load")) return;
    await page.click(".category-tab.mixes");
    await page.waitForTimeout(3000);

    const ws = await createAudioWs(WS_URL, token!);
    try {
      ws.send(
        JSON.stringify({
          cmd: "ListenStart",
          member_id: memberSends[0].name.toLowerCase(),
        }),
      );
      await page.waitForTimeout(2000);
      ws.send(JSON.stringify({ cmd: "ListenStop" }));
      await page.waitForTimeout(6000);

      const uiStates = await getUiMuteStates(page);

      for (const ms of memberSends) {
        const reaperMuted = await getReaperSendMute(
          page.request,
          ms.trackIdx,
          ms.sendIdx,
        );
        const uiCh = uiStates.find((ch) => ch.name.toUpperCase() === ms.name);
        if (uiCh) {
          expect(
            uiCh.isMuted,
            `${ms.name}: UI=${uiCh.isMuted}, REAPER=${reaperMuted} — DESYNC!`,
          ).toBe(reaperMuted);
        }
      }
    } finally {
      ws.close();
    }
  });

  test("Bug 3: UI mute via click preserved after listen (persistent WS)", async ({
    page,
  }) => {
    const token = await getAuthToken(page.request);
    if (!assume(token, "Auth must succeed")) return;

    const info = await findMemberSendsToEngineer(page.request);
    if (!assume(info && info.memberSends.length >= 2, "Need 2+ member sends"))
      return;
    const { memberSends } = info!;

    const originalMutes: Record<string, boolean> = {};
    for (const ms of memberSends) {
      originalMutes[ms.name] = await getReaperSendMute(
        page.request,
        ms.trackIdx,
        ms.sendIdx,
      );
    }

    try {
      await page.goto("/");
      await loginAs(page, "engineer", "1177");
      await page.goto("/engineer");
      const mixerLoaded = await page
        .waitForSelector(".app.mixer", { timeout: 10000 })
        .catch(() => null);
      if (!assume(mixerLoaded, "Mixer must load")) return;
      await page.click(".category-tab.mixes");
      await page.waitForTimeout(3000);

      // Click mute on first member channel via UI
      const targetName = memberSends[0].name;
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

      // Ensure starts unmuted
      const startClasses = await muteBtn.getAttribute("class");
      if (startClasses?.includes(" on")) {
        await muteBtn.click();
        await page.waitForTimeout(1000);
      }
      // Mute via UI click
      await muteBtn.click();
      await page.waitForTimeout(1500);

      const afterMuteClasses = await muteBtn.getAttribute("class");
      console.log(`AFTER UI MUTE: ${targetName} classes="${afterMuteClasses}"`);
      expect(
        afterMuteClasses,
        `${targetName} should be muted after click`,
      ).toContain(" on");

      // Listen cycle via WS
      const ws = await createAudioWs(WS_URL, token!);
      try {
        ws.send(
          JSON.stringify({
            cmd: "ListenStart",
            member_id: memberSends[1].name.toLowerCase(),
          }),
        );
        await page.waitForTimeout(2000);

        const duringClasses = await muteBtn.getAttribute("class");
        console.log(`DURING LISTEN: ${targetName} classes="${duringClasses}"`);

        ws.send(JSON.stringify({ cmd: "ListenStop" }));
        await page.waitForTimeout(6000);

        const afterListenClasses = await muteBtn.getAttribute("class");
        console.log(
          `AFTER LISTEN STOP: ${targetName} classes="${afterListenClasses}"`,
        );

        expect(
          afterListenClasses,
          `${targetName}: mute should STILL show muted after listen — BUG 3!`,
        ).toContain(" on");

        const reaperMuted = await getReaperSendMute(
          page.request,
          memberSends[0].trackIdx,
          memberSends[0].sendIdx,
        );
        console.log(`REAPER: ${targetName} muted=${reaperMuted}`);
        expect(reaperMuted, `${targetName} REAPER must be muted`).toBe(true);
      } finally {
        ws.close();
      }
    } finally {
      await page.request.get(
        "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/listen_mute_backup/",
      );
      for (const ms of memberSends) {
        await page.request.get(
          `http://iem.lan:8080/_/SET/TRACK/${ms.trackIdx}/SEND/${ms.sendIdx}/MUTE/${originalMutes[ms.name] ? 1 : 0}`,
        );
      }
    }
  });

  test("Bug 4: Rapid listen toggles preserve mute state (persistent WS)", async ({
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

    const ws = await createAudioWs(WS_URL, token!);
    try {
      const memberId = info!.memberSends[0].name.toLowerCase();
      for (let i = 0; i < 3; i++) {
        ws.send(JSON.stringify({ cmd: "ListenStart", member_id: memberId }));
        await page.waitForTimeout(300);
        ws.send(JSON.stringify({ cmd: "ListenStop" }));
        await page.waitForTimeout(300);
      }

      await page.waitForTimeout(8000);

      const afterRapid = await getUiMuteStates(page);
      console.log("AFTER RAPID TOGGLE:", JSON.stringify(afterRapid));

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
