import { test, expect, Page } from "@playwright/test";
import * as path from "path";

const REAPER_URL = "http://iem.lan:8080";
const FIXTURE_PATH = path.resolve(
  __dirname,
  "../fixtures/talkback-1k-tone.wav",
);

// Chromium flags that feed our fixture WAV into getUserMedia.
// WebCodecs AudioEncoder requires a secure context (HTTPS / localhost).
// When E2E_BASE_URL is a plain-http LAN IP (e.g. http://10.77.9.231),
// Chromium refuses WebCodecs unless we explicitly mark the origin secure.
// Listing both the LAN IP and localhost covers CI (localhost on runner)
// and dev-machine ad-hoc runs.
const BASE_URL = process.env.E2E_BASE_URL || "http://localhost";
const SECURE_ORIGINS = [BASE_URL, "http://10.77.9.231", "http://localhost"].join(
  ",",
);
const FAKE_MIC_ARGS = [
  "--use-fake-ui-for-media-stream",
  "--use-fake-device-for-media-stream",
  `--use-file-for-fake-audio-capture=${FIXTURE_PATH}`,
  `--unsafely-treat-insecure-origin-as-secure=${SECURE_ORIGINS}`,
];

async function loginAs(page: Page, member: string, pin: string) {
  const response = await page.request.post("/api/auth", {
    data: { member, pin },
  });
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

async function readEngineerMeterDb10(
  page: Page,
): Promise<number | null> {
  // REAPER returns TRACK lines; field 6 is last_meter_peak (dB * 10).
  // ENGINEER mic track has name "ENGINEER mic" — find by name.
  const resp = await page.request.get(`${REAPER_URL}/_/NTRACK;TRACK`);
  if (resp.status() !== 200) return null;
  const body = await resp.text();
  for (const line of body.split("\n")) {
    if (!line.startsWith("TRACK\t")) continue;
    const fields = line.split("\t");
    // fields[2] = name, fields[6] = last_meter_peak (dB*10)
    if (fields.length < 7) continue;
    if (/^ENGINEER\s+mic$/i.test(fields[2])) {
      const v = parseInt(fields[6], 10);
      return Number.isFinite(v) ? v : null;
    }
  }
  return null;
}

// Move launchOptions to top-level (describe-level forces a new worker, which is forbidden)
test.use({
  launchOptions: { args: FAKE_MIC_ARGS },
  permissions: ["microphone"],
});

test.describe("#154 Talkback audio quality (live)", () => {
  const consoleMessages: string[] = [];

  test.beforeEach(async ({ page }) => {
    consoleMessages.length = 0;
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        if (msg.text().includes("subscribe await failed")) return;
        if (msg.text().includes("Push API in incognito")) return;
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });
  });

  test.afterEach(async () => {
    const real = consoleMessages.filter(
      (m) =>
        !m.includes("[vite]") &&
        !m.includes("favicon") &&
        !m.includes("integrity") &&
        !m.includes("WebSocket connection") &&
        !m.includes("navigator.vibrate"),
    );
    expect(real).toEqual([]);
  });

  test("engineer talkback delivers continuous signal to ENGINEER mic track", async ({
    page,
  }) => {
    test.setTimeout(60_000);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await expect(page.locator(".mixer-header").first()).toBeVisible({
      timeout: 10_000,
    });

    const talkBtn = page.locator(".toolbar-btn-talk");
    await expect(talkBtn).toBeVisible({ timeout: 10_000 });

    // If the button renders as "unsupported" (e.g. WebCodecs unavailable
    // because the origin is not a secure context), fail loudly — the test
    // setup is wrong rather than the feature being broken.
    const btnClass = await talkBtn.getAttribute("class");
    expect(
      btnClass,
      `setup error: TalkButton rendered as '${btnClass}' — WebCodecs likely blocked. ` +
        "Run against http://localhost on a secure-context-supporting origin.",
    ).not.toContain("unsupported");

    // Use a real mouse press — Leptos `on:pointerdown` needs a natural
    // event with a valid pointer_id so set_pointer_capture succeeds.
    // After pointerdown, wait 2.5 s for the WS + getUserMedia + Opus
    // encoder + REAPER VST UDP registration to settle before sampling.
    await talkBtn.hover();
    await page.mouse.down();
    await page.waitForTimeout(2500);

    const samples: number[] = [];
    const POLL_MS = 100;
    const POLL_COUNT = 50; // 5 s

    for (let i = 0; i < POLL_COUNT; i++) {
      await page.waitForTimeout(POLL_MS);
      const db10 = await readEngineerMeterDb10(page);
      samples.push(db10 ?? -1500);
    }

    await page.mouse.up();

    // Wait up to 500 ms for meter to decay post-release.
    const releaseSamples: number[] = [];
    for (let i = 0; i < 5; i++) {
      await page.waitForTimeout(100);
      const db10 = await readEngineerMeterDb10(page);
      releaseSamples.push(db10 ?? -1500);
    }

    // A1 — Signal present: >= 40 of 50 samples above -60 dB (-600 in dB*10).
    const aboveSilence = samples.filter((v) => v > -600).length;
    expect(
      aboveSilence,
      `A1 FAIL: only ${aboveSilence}/50 samples above -60 dB during talk. samples=${JSON.stringify(samples)}`,
    ).toBeGreaterThanOrEqual(40);

    // A2 — No hang: no consecutive 500 ms (5 samples) block of silence during talk.
    let worstRun = 0;
    let run = 0;
    for (const v of samples) {
      if (v <= -600) {
        run++;
        if (run > worstRun) worstRun = run;
      } else {
        run = 0;
      }
    }
    expect(
      worstRun,
      `A2 FAIL: longest silent run during talk = ${worstRun} x 100 ms; must be < 5`,
    ).toBeLessThan(5);

    // A3 — Clean release: meter <= -60 dB within 200 ms (2 samples) after release.
    const quickRelease = releaseSamples.slice(0, 2).every((v) => v <= -600);
    expect(
      quickRelease,
      `A3 FAIL: meter did not decay within 200 ms. releaseSamples=${JSON.stringify(releaseSamples)}`,
    ).toBe(true);

    // A4 — Diagnostics API returns the new schema with sane counters.
    // Note: on v1.147.0 production this assertion fails because
    // packets_in is not yet part of the response. Task 7 of the fix
    // introduces the full schema and adds a Bearer token to this block.
    const diagResp = await page.request.get("/api/talkback/diagnostics");
    expect(diagResp.status(), "A4 FAIL: /api/talkback/diagnostics not 200").toBe(
      200,
    );
    const diag = await diagResp.json();
    expect(diag.packets_in, `A4 FAIL: packets_in missing or too low: ${JSON.stringify(diag)}`).toBeGreaterThan(200);
    expect(diag.packets_out, `A4 FAIL: packets_out too low: ${JSON.stringify(diag)}`).toBeGreaterThan(200);
    expect(diag.seq_gaps, `A4 FAIL: seq_gaps should be 0: ${JSON.stringify(diag)}`).toBe(0);
    expect(diag.buffer_overflows, `A4 FAIL: buffer_overflows should be 0 on loopback: ${JSON.stringify(diag)}`).toBe(0);
    expect(diag.recv_vst_addr, `A4 FAIL: recv_vst_addr null: ${JSON.stringify(diag)}`).toBeTruthy();
    expect(diag.recv_vst_addr).not.toBe("none");
  });
});
