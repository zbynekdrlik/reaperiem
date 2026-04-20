import { test, expect, Page } from "@playwright/test";

// Login helper matching alex-kl.spec.ts / stems-volume.spec.ts convention
async function loginAs(page: Page, member: string) {
  const response = await page.request.post("/api/auth", {
    data: { member, pin: "7711" },
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
  await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible({
    timeout: 10000,
  });
}

// Resolve CG's REAPER track index and its STEVO-inear send index
// directly from REAPER HTTP so restore is robust to track reordering.
async function resolveCgStevoSend(
  request: import("@playwright/test").APIRequestContext,
) {
  const tracksResp = await request.get("http://iem.lan:8080/_/NTRACK;TRACK");
  const tracksText = await tracksResp.text();
  const lines = tracksText.split("\n");
  const findRow = (needle: string) =>
    lines.find((l) => {
      const parts = l.split("\t");
      return parts[0] === "TRACK" && parts[2] === needle;
    });
  const cgRow = findRow("CG");
  const stevoInearRow = findRow("STEVO inear");
  expect(cgRow, "CG track not found in REAPER").toBeTruthy();
  expect(stevoInearRow, "STEVO inear track not found in REAPER").toBeTruthy();
  const cgIdx = parseInt(cgRow!.split("\t")[1], 10);
  const stevoInearIdx = parseInt(stevoInearRow!.split("\t")[1], 10);

  let stevoSendIdx = -1;
  let muteBefore = 0;
  let volBefore = 1.0;
  for (let s = 0; s < 20; s++) {
    const r = await request.get(
      `http://iem.lan:8080/_/GET/TRACK/${cgIdx}/SEND/${s}`,
    );
    const line = (await r.text()).trim();
    if (!line.startsWith("SEND")) break;
    const p = line.split("\t");
    if (p.length >= 7 && parseInt(p[6], 10) === stevoInearIdx) {
      stevoSendIdx = s;
      muteBefore = parseInt(p[3], 10);
      volBefore = parseFloat(p[4]);
      break;
    }
  }
  expect(
    stevoSendIdx,
    "CG → STEVO inear send not found",
  ).toBeGreaterThanOrEqual(0);

  return { cgIdx, stevoInearIdx, stevoSendIdx, muteBefore, volBefore };
}

test.describe("CG (stereo content-playback input)", () => {
  test("appears in the Tech tab as a single stereo channel", async ({
    page,
  }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        consoleErrors.push(`[error] ${msg.text()}`);
      }
    });

    await page.goto("/");
    await loginAs(page, "stevo");
    await page.goto("/stevo");
    await waitForMixer(page);

    // Navigate to Tech tab
    const techTab = page.locator("text=Tech").first();
    await expect(techTab).toBeVisible({ timeout: 10000 });
    await techTab.click();
    await page.waitForTimeout(200);

    // CG is a single-word name — only .ch-name matches, no .ch-type suffix
    // (unlike "ALEX kl" which splits ALEX/kl).
    const cg = page
      .locator(".channel")
      .filter({ has: page.locator(".ch-name", { hasText: /^CG$/ }) });
    await expect(cg).toHaveCount(1, { timeout: 10000 });
    await expect(cg.first()).toBeVisible();

    expect(consoleErrors).toEqual([]);
  });

  test("dragging the CG fader changes REAPER send level", async ({
    page,
    request,
  }) => {
    const {
      cgIdx,
      stevoSendIdx,
      muteBefore,
      volBefore,
    } = await resolveCgStevoSend(request);

    // CG sends are default-muted (mute flag = 8). To observe a fader-level
    // change we must UNMUTE the send first. Restore muteBefore in finally.
    if (muteBefore !== 0) {
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/MUTE/0`,
      );
    }

    try {
      await page.goto("/");
      await loginAs(page, "stevo");
      await page.goto("/stevo");
      await waitForMixer(page);

      const techTab = page.locator("text=Tech").first();
      await expect(techTab).toBeVisible({ timeout: 10000 });
      await techTab.click();
      await page.waitForTimeout(200);

      const cg = page
        .locator(".channel")
        .filter({ has: page.locator(".ch-name", { hasText: /^CG$/ }) })
        .first();
      await expect(cg).toBeVisible({ timeout: 10000 });

      const dbLabel = cg.locator(".db-display");
      const parseDb = async () => {
        const txt = (await dbLabel.textContent()) || "0";
        if (/[\u221E]|inf/i.test(txt)) return -Infinity;
        return parseFloat(txt.replace(/[^-\d.]/g, ""));
      };
      const dbStart = await parseDb();

      const fader = cg.locator(".fader-track");
      const box = await fader.boundingBox();
      expect(box).not.toBeNull();

      // Incremental drag from 70% to 30% — single-jump moves don't trigger
      // pointer events on this component (matches alex-kl.spec.ts pattern).
      const startX = box!.x + box!.width * 0.7;
      const endX = box!.x + box!.width * 0.3;
      const y = box!.y + box!.height / 2;

      await page.mouse.move(startX, y);
      await page.mouse.down();
      await page.waitForTimeout(200);
      const steps = 10;
      for (let i = 1; i <= steps; i++) {
        await page.mouse.move(startX + (endX - startX) * (i / steps), y);
        await page.waitForTimeout(40);
      }
      await page.mouse.up();
      await page.waitForTimeout(500);

      // PRIMARY: verify REAPER actually received the change via HTTP roundtrip.
      // UI .db-display is optimistic (updates client-side during drag whether
      // or not WS command was applied) — PR #180 lesson.
      const postDragResp = await request.get(
        `http://iem.lan:8080/_/GET/TRACK/${cgIdx}/SEND/${stevoSendIdx}`,
      );
      const postDragParts = (await postDragResp.text()).trim().split("\t");
      const postDragVol = parseFloat(postDragParts[4]);
      expect(
        postDragVol,
        `REAPER send D_VOL must drop after UI drag — was ${volBefore}, got ${postDragVol}. If unchanged, UI command did not reach REAPER.`,
      ).toBeLessThan(volBefore * 0.7);

      // SECONDARY: UI reflects the same state
      const dbEnd = await parseDb();
      expect(dbEnd).toBeLessThan(dbStart - 3);
      expect(dbEnd).toBeLessThan(0);
    } finally {
      // Restore starting state — both volume and mute — so this live test
      // doesn't leak state between runs.
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/VOL/${volBefore}`,
      );
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/MUTE/${muteBefore}`,
      );
    }
  });

  test("muting CG mutes the REAPER send — not just the UI", async ({
    page,
    request,
  }) => {
    const { cgIdx, stevoSendIdx, muteBefore } = await resolveCgStevoSend(
      request,
    );

    // Starting state for CG is typically MUTED (flag=8). To observe a mute
    // transition we must UNMUTE first, then click the UI mute button,
    // then assert the send is muted again.
    if (muteBefore !== 0) {
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/MUTE/0`,
      );
    }

    try {
      await page.goto("/");
      await loginAs(page, "stevo");
      await page.goto("/stevo");
      await waitForMixer(page);

      const techTab = page.locator("text=Tech").first();
      await expect(techTab).toBeVisible({ timeout: 10000 });
      await techTab.click();
      await page.waitForTimeout(200);

      const cg = page
        .locator(".channel")
        .filter({ has: page.locator(".ch-name", { hasText: /^CG$/ }) })
        .first();
      await expect(cg).toBeVisible({ timeout: 10000 });

      const muteBtn = cg.locator(".mute-btn").first();
      await expect(muteBtn).toBeVisible();
      await muteBtn.click();
      await page.waitForTimeout(700); // WS roundtrip + REAPER apply

      // Authoritative check: REAPER send mute flag must be 8 (muted).
      const afterResp = await request.get(
        `http://iem.lan:8080/_/GET/TRACK/${cgIdx}/SEND/${stevoSendIdx}`,
      );
      const afterParts = (await afterResp.text()).trim().split("\t");
      const muteAfter = parseInt(afterParts[3], 10);
      expect(
        muteAfter,
        "REAPER send MUTE flag must be 8 (muted) after UI mute click. If 0, command did not reach REAPER.",
      ).toBe(8);
    } finally {
      // Restore the original mute state (likely muted=8 for CG default)
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/MUTE/${muteBefore}`,
      );
    }
  });
});
