import { test, expect, Page } from "@playwright/test";

// Login helper matching stems-volume.spec.ts / mixer.spec.ts convention
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

test.describe("ALEX kl (keyboard stereo input)", () => {
  test("appears in the Mics tab as a single stereo channel", async ({
    page,
  }) => {
    // Collect console errors for zero-error assertion
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

    // Navigate to Mics tab (may already be default)
    const micsTab = page.locator("text=Mics").first();
    if ((await micsTab.count()) > 0) {
      await micsTab.click();
      await page.waitForTimeout(200);
    }

    // Assert the channel exists with exact name "ALEX kl"
    // The UI splits track names: ".ch-name" shows the first word ("ALEX")
    // and ".ch-type" shows the instrument suffix ("kl"). Match both.
    const alexKl = page
      .locator(".channel")
      .filter({ has: page.locator(".ch-name", { hasText: /^ALEX$/ }) })
      .filter({ has: page.locator(".ch-type", { hasText: /^kl/ }) });
    await expect(alexKl).toHaveCount(1, { timeout: 10000 });
    await expect(alexKl.first()).toBeVisible();

    // Console must be clean for the feature to count as working
    expect(consoleErrors).toEqual([]);
  });

  test("dragging the ALEX kl fader changes REAPER send level", async ({
    page,
    request,
  }) => {
    await page.goto("/");
    await loginAs(page, "stevo");
    await page.goto("/stevo");
    await waitForMixer(page);

    const micsTab = page.locator("text=Mics").first();
    if ((await micsTab.count()) > 0) {
      await micsTab.click();
      await page.waitForTimeout(200);
    }

    // Locate the ALEX kl channel (split as ch-name="ALEX" + ch-type="kl")
    const alexKl = page
      .locator(".channel")
      .filter({ has: page.locator(".ch-name", { hasText: /^ALEX$/ }) })
      .filter({ has: page.locator(".ch-type", { hasText: /^kl/ }) })
      .first();
    await expect(alexKl).toBeVisible({ timeout: 10000 });

    // Read starting dB so we can assert relative change after the drag.
    const dbLabel = alexKl.locator(".db-display");
    const parseDb = async () => {
      const txt = (await dbLabel.textContent()) || "0";
      // Normalise "-∞" and "-inf" variants before parseFloat
      if (/[\u221E]|inf/i.test(txt)) return -Infinity;
      return parseFloat(txt.replace(/[^-\d.]/g, ""));
    };
    const dbStart = await parseDb();

    const fader = alexKl.locator(".fader-track");
    const box = await fader.boundingBox();
    expect(box).not.toBeNull();

    // Drag from ~70% of fader to ~30% — incremental moves are required,
    // single-jump moves don't trigger pointer events on this component.
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

    // Verify the drag produced a measurable dB drop via the WebSocket
    // round-trip (.db-display reflects the poller's snapshot of REAPER).
    const dbEnd = await parseDb();
    // Moving left on the fader lowers dB. A 40%-of-width drag must produce
    // at least a 3 dB drop — anything less means the drag wasn't registered
    // or the WS round-trip didn't propagate the change.
    expect(dbEnd).toBeLessThan(dbStart - 3);
    expect(dbEnd).toBeLessThan(0);
  });
});
