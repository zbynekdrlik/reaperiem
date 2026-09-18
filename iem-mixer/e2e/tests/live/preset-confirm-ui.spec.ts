/**
 * #206 — preset / history UI: explicit Slovak buttons + confirmation before
 * overwrite and delete.
 *
 * Before the fix, LOAD was an unlabelled click on the row text and the write
 * actions were cryptic `Upd`/`Del` that fired immediately with no confirmation
 * (a mis-tap silently overwrote or deleted). This test asserts the Slovak
 * labels and that overwrite/delete open a confirmation dialog which, when
 * cancelled, changes nothing.
 *
 * Safety: member login (petronela, PIN 7711). The test only creates+deletes a
 * probe preset FILE and exercises CANCEL paths — it never loads/restores, so no
 * REAPER send/EQ is ever written. The probe preset is removed in `finally`.
 */

import { test, expect, Page } from "@playwright/test";

const MEMBER = "petronela";
const PIN = "7711";
const PROBE = "e2e_206_confirm_probe";

async function authToken(page: Page): Promise<string> {
  const resp = await page.request.post("/api/auth", {
    data: { member: MEMBER, pin: PIN },
  });
  expect(resp.status()).toBe(200);
  return (await resp.json()).token as string;
}

test.describe.configure({ timeout: 120_000 });

test.describe("#206 preset/history confirmation UI", () => {
  test("Slovak labels + overwrite/delete confirm dialogs (cancel = no change)", async ({
    page,
  }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (text.includes("apple-mobile-web-app-capable")) return;
        if (text.includes("[push] subscribe await failed")) return;
        if (text.includes("Push API in incognito mode")) return;
        if (/integrity.*attribute.*ignored/i.test(text)) return;
        if (text.includes("vapid-key fetch error")) return;
        consoleErrors.push(`[${msg.type()}] ${text}`);
      }
    });

    await page.goto("/");
    const token = await authToken(page);
    const headers = { Authorization: `Bearer ${token}` };

    try {
      // Pre-create a probe preset FILE (no REAPER writes).
      const createResp = await page.request.post(`/api/presets/${MEMBER}`, {
        headers,
        data: {
          name: PROBE,
          channels: { "5": { vol: -6.0, mute: false, pan: 0.5 } },
        },
      });
      expect(
        createResp.status(),
        `probe preset create failed: ${await createResp.text()}`,
      ).toBe(201);

      const readUpdatedAt = async (): Promise<number> => {
        const r = await page.request.get(
          `/api/presets/${MEMBER}/${encodeURIComponent(PROBE)}`,
          { headers },
        );
        expect(r.ok()).toBeTruthy();
        return (await r.json()).updated_at as number;
      };
      const originalUpdatedAt = await readUpdatedAt();

      // Log the member in and open the mixer.
      await page.evaluate(
        ({ token }) => {
          localStorage.setItem(
            "iem_token",
            JSON.stringify({ token, member: "petronela", engineer: false }),
          );
        },
        { token },
      );
      await page.goto(`/${MEMBER}`);
      await expect(
        page.locator(".app.mixer, .mixer-header").first(),
      ).toBeVisible({ timeout: 10000 });

      // Open the Presets modal.
      await page.locator("button", { hasText: "Presets" }).click();
      const modal = page.locator(".modal-overlay.visible .modal");
      await expect(modal).toBeVisible({ timeout: 3000 });

      const probeRow = modal.locator(".preset-item", { hasText: PROBE });
      await expect(probeRow).toBeVisible({ timeout: 5000 });

      // #206: explicit Slovak row buttons + bottom button.
      await expect(probeRow.locator(".load-preset")).toHaveText("Načítať");
      await expect(probeRow.locator(".update-preset")).toHaveText("Prepísať");
      await expect(probeRow.locator(".delete-preset")).toHaveText("Zmazať");
      await expect(modal.locator(".preset-save-btn")).toHaveText(
        "Uložiť ako nový",
      );

      // Save under the existing name → overwrite confirm dialog → cancel.
      await modal.locator(".preset-input").fill(PROBE);
      await modal.locator(".preset-save-btn").click();
      const confirm = page.locator(".confirm-overlay.visible");
      await expect(confirm).toBeVisible({ timeout: 3000 });
      await expect(confirm.locator(".confirm-title")).toHaveText(
        "Prepísať preset?",
      );
      await confirm.locator(".confirm-btn-cancel").click();
      await expect(confirm).toBeHidden();
      // Cancelling must NOT overwrite the preset.
      expect(await readUpdatedAt()).toBe(originalUpdatedAt);

      // Delete → confirm dialog → cancel → preset still present.
      await probeRow.locator(".delete-preset").click();
      await expect(confirm).toBeVisible({ timeout: 3000 });
      await expect(confirm.locator(".confirm-title")).toHaveText(
        "Zmazať preset?",
      );
      await confirm.locator(".confirm-btn-cancel").click();
      await expect(confirm).toBeHidden();
      await expect(probeRow).toBeVisible();

      // Close Presets, open History → Slovak verbs + bottom button.
      await modal.locator(".modal-close").click();
      await page.locator("button", { hasText: "History" }).click();
      const history = page.locator(".modal-overlay.visible .modal.snapshot-modal");
      await expect(history).toBeVisible({ timeout: 3000 });
      await expect(history.locator(".snapshot-save-btn")).toHaveText(
        "Uložiť teraz",
      );
      await expect(history.locator("h2")).toHaveText("História mixu");
      // If any snapshot rows exist, they carry the Slovak verbs + pin toggle.
      const firstRow = history.locator(".snapshot-item").first();
      if ((await history.locator(".snapshot-item").count()) > 0) {
        await expect(firstRow.locator(".restore-btn")).toHaveText("Obnoviť");
        await expect(firstRow.locator(".delete-btn")).toHaveText("Zmazať");
        await expect(firstRow.locator(".snapshot-pin-btn")).toHaveCount(1);
      }
    } finally {
      const token2 = await authToken(page);
      await page.request
        .delete(`/api/presets/${MEMBER}/${encodeURIComponent(PROBE)}`, {
          headers: { Authorization: `Bearer ${token2}` },
        })
        .catch(() => {});
    }

    expect(consoleErrors).toEqual([]);
  });
});
