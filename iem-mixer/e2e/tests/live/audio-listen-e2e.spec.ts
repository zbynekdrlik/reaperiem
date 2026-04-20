/**
 * #179.5 — Binary-frames-or-die E2E test for the Listen button.
 *
 * Opens /ws/audio as engineer, sends ListenStart, counts binary Opus frames
 * received over 3 seconds. The tone generator is active during post-deploy
 * E2E (see ci.yml), so the audio pipeline MUST deliver frames. Any silence
 * is a regression.
 *
 * This test is intentionally minimal and zero-tolerance — no try/catch,
 * no silent skips, no tolerance of "no_source".
 */

import { test, expect, Page } from "@playwright/test";

async function loginAsEngineer(page: Page): Promise<void> {
  const response = await page.request.post("/api/auth", {
    data: { member: "engineer", pin: "1177" },
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

async function probeWsAudio(
  page: Page,
  memberId: string,
  probeMs: number,
): Promise<{
  binCount: number;
  totalBytes: number;
  firstBinLatency: number | null;
  textMsgs: string[];
}> {
  return await page.evaluate(
    async ({ memberId, probeMs }) => {
      const auth = JSON.parse(localStorage.getItem("iem_token")!);
      const proto = location.protocol === "https:" ? "wss:" : "ws:";
      const url = `${proto}//${location.host}/ws/audio?token=${auth.token}`;
      const ws = new WebSocket(url);
      ws.binaryType = "arraybuffer";

      let binCount = 0;
      let totalBytes = 0;
      let firstBinMs: number | null = null;
      const textMsgs: string[] = [];

      ws.onmessage = (e: MessageEvent) => {
        if (e.data instanceof ArrayBuffer) {
          if (firstBinMs === null) firstBinMs = Date.now();
          binCount++;
          totalBytes += e.data.byteLength;
        } else if (typeof e.data === "string") {
          textMsgs.push(e.data);
        }
      };

      await new Promise<void>((res, rej) => {
        ws.onopen = () => res();
        ws.onerror = () => rej(new Error("ws connect failed"));
        setTimeout(() => rej(new Error("ws open timeout")), 3000);
      });

      const sentAt = Date.now();
      ws.send(JSON.stringify({ cmd: "ListenStart", member_id: memberId }));

      try {
        await new Promise((r) => setTimeout(r, probeMs));
      } finally {
        // Production-safe: always send ListenStop so the server restores
        // any saved member mute state (belt-and-suspenders with WS-disconnect
        // cleanup) per MEMORY feedback_live_test_safety.md.
        try {
          ws.send(JSON.stringify({ cmd: "ListenStop" }));
        } catch {
          /* send may fail if socket is already closed; disconnect cleanup covers us */
        }
        ws.close();
      }

      return {
        binCount,
        totalBytes,
        firstBinLatency: firstBinMs !== null ? firstBinMs - sentAt : null,
        textMsgs,
      };
    },
    { memberId, probeMs },
  );
}

test.describe("Listen /ws/audio binary-frames-or-die", () => {
  test("engineer ListenStart delivers binary Opus frames within 1s and >=30 frames in 3s", async ({
    page,
  }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (text.includes("apple-mobile-web-app-capable")) return;
        if (text.includes("[push] subscribe await failed")) return;
        if (text.includes("Push API in incognito mode")) return;
        if (/integrity.*attribute.*ignored/i.test(text)) return;
        if (text.includes("vapid-key fetch error")) return;
        consoleMessages.push(`[${msg.type()}] ${text}`);
      }
    });

    await page.goto("/");
    await loginAsEngineer(page);
    await page.goto("/engineer");
    await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible(
      { timeout: 10000 },
    );

    const result = await probeWsAudio(page, "engineer", 3000);

    // Hard assertions — zero tolerance
    expect(result.textMsgs.some((m) => m.includes('"status":"listening"'))).toBe(
      true,
    );
    expect(result.textMsgs.some((m) => m.includes('"status":"no_source"'))).toBe(
      false,
    );
    expect(result.binCount).toBeGreaterThanOrEqual(30);
    expect(result.totalBytes).toBeGreaterThan(1000);
    expect(result.firstBinLatency).not.toBeNull();
    expect(result.firstBinLatency!).toBeLessThan(1000);

    expect(consoleMessages).toEqual([]);
  });

  test("engineer ListenStart member_id=petronela delivers binary Opus frames (solo-mute path)", async ({
    page,
  }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (text.includes("apple-mobile-web-app-capable")) return;
        if (text.includes("[push] subscribe await failed")) return;
        if (text.includes("Push API in incognito mode")) return;
        if (/integrity.*attribute.*ignored/i.test(text)) return;
        if (text.includes("vapid-key fetch error")) return;
        consoleMessages.push(`[${msg.type()}] ${text}`);
      }
    });

    await page.goto("/");
    await loginAsEngineer(page);
    await page.goto("/engineer");
    await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible(
      { timeout: 10000 },
    );

    // probeWsAudio handles the ListenStop-in-finally production-safety contract
    const result = await probeWsAudio(page, "petronela", 3000);

    expect(result.textMsgs.some((m) => m.includes('"status":"listening"'))).toBe(
      true,
    );
    expect(result.textMsgs.some((m) => m.includes('"status":"no_source"'))).toBe(
      false,
    );
    expect(result.binCount).toBeGreaterThanOrEqual(30);
    expect(result.totalBytes).toBeGreaterThan(1000);
    expect(result.firstBinLatency).not.toBeNull();
    expect(result.firstBinLatency!).toBeLessThan(1000);

    expect(consoleMessages).toEqual([]);
  });
});
