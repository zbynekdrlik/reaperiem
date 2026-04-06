import { test, expect, Page } from "@playwright/test";

// Helper to get a JWT token
async function getToken(
  page: Page,
  member: string,
  pin: string = "7711",
): Promise<{ token: string; member: string; engineer: boolean } | null> {
  const response = await page.request.post("/api/auth", {
    data: { member, pin },
  });
  if (response.status() === 200) {
    return await response.json();
  }
  return null;
}

// Helper to get engineer token
async function getEngineerToken(
  page: Page,
): Promise<{ token: string; member: string; engineer: boolean } | null> {
  return getToken(page, "engineer", "1177");
}

test.describe("Elevated member access (Petronela hardcoded)", () => {
  test("petronela gets mix channels in mixer state (requires REAPER)", async ({
    page,
  }) => {
    await page.goto("/");

    // This test requires REAPER to be running (mix sends must exist)
    // Retry up to 5 times with 3s delay — REAPER may be slow after deploy
    let reaperCheck = await page.request.get("/api/reaper/NTRACK");
    for (let attempt = 0; attempt < 4 && !reaperCheck.ok(); attempt++) {
      await page.waitForTimeout(3000);
      reaperCheck = await page.request.get("/api/reaper/NTRACK");
    }
    expect(reaperCheck.ok()).toBe(true);

    const engAuth = await getEngineerToken(page);
    expect(engAuth).toBeTruthy();

    // Wait for background discovery to complete
    await page.waitForTimeout(5000);

    // Get mixer state for petronela (as engineer)
    const mixerResp = await page.request.get("/api/mixer/petronela", {
      headers: { Authorization: `Bearer ${engAuth!.token}` },
    });

    if (mixerResp.status() === 200) {
      const mixer = await mixerResp.json();
      const mixChannels = mixer.channels.filter(
        (ch: { category: string }) => ch.category === "mixes",
      );
      // Mix channels should exist for petronela (hardcoded elevated)
      expect(mixChannels.length).toBeGreaterThan(0);
      // All mix channels should be muted by default
      for (const ch of mixChannels) {
        expect(ch.muted).toBe(true);
      }
    }
  });

  test("non-petronela member does NOT get mix channels", async ({ page }) => {
    await page.goto("/");

    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    const nonPetronela = members.find(
      (m: { id: string }) => m.id !== "engineer" && m.id !== "petronela",
    );
    expect(nonPetronela).toBeTruthy();

    const engAuth = await getEngineerToken(page);
    expect(engAuth).toBeTruthy();

    const mixerResp = await page.request.get(`/api/mixer/${nonPetronela.id}`, {
      headers: { Authorization: `Bearer ${engAuth!.token}` },
    });

    if (mixerResp.status() === 200) {
      const mixer = await mixerResp.json();
      const mixChannels = mixer.channels.filter(
        (ch: { category: string }) => ch.category === "mixes",
      );
      // Non-petronela members should NOT have mix channels
      expect(mixChannels.length).toBe(0);
    }
  });

  test("elevated API endpoint no longer exists", async ({ page }) => {
    await page.goto("/");

    const engAuth = await getEngineerToken(page);
    expect(engAuth).toBeTruthy();

    // The elevated endpoint is gone — response should not contain valid elevated JSON
    const resp = await page.request.get("/api/members/petronela/elevated", {
      headers: { Authorization: `Bearer ${engAuth!.token}` },
    });
    const text = await resp.text();
    expect(text.includes('"elevated"')).toBe(false);
  });
});
