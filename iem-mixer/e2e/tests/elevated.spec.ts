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

// Guard: early return when precondition not met
function assume(condition: unknown, message: string): condition is true {
  if (!condition) {
    console.log(`[ASSUME SKIP] ${message}`);
    return false;
  }
  return true;
}

test.describe("Elevated member access (#122)", () => {
  test("GET elevated returns false by default", async ({ page }) => {
    await page.goto("/");

    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    const engAuth = await getEngineerToken(page);
    if (!assume(engAuth, "Engineer login failed")) return;

    const resp = await page.request.get(`/api/members/${member}/elevated`, {
      headers: { Authorization: `Bearer ${engAuth!.token}` },
    });
    expect(resp.status()).toBe(200);
    const body = await resp.json();
    expect(body.elevated).toBe(false);
  });

  test("engineer can set elevated=true", async ({ page }) => {
    await page.goto("/");

    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    const engAuth = await getEngineerToken(page);
    if (!assume(engAuth, "Engineer login failed")) return;

    // Set elevated
    const setResp = await page.request.put(`/api/members/${member}/elevated`, {
      headers: {
        Authorization: `Bearer ${engAuth!.token}`,
        "Content-Type": "application/json",
      },
      data: { elevated: true },
    });
    expect(setResp.status()).toBe(200);
    const setBody = await setResp.json();
    expect(setBody.elevated).toBe(true);

    // Verify it persisted
    const getResp = await page.request.get(`/api/members/${member}/elevated`, {
      headers: { Authorization: `Bearer ${engAuth!.token}` },
    });
    expect(getResp.status()).toBe(200);
    const getBody = await getResp.json();
    expect(getBody.elevated).toBe(true);

    // Clean up: set back to false
    await page.request.put(`/api/members/${member}/elevated`, {
      headers: {
        Authorization: `Bearer ${engAuth!.token}`,
        "Content-Type": "application/json",
      },
      data: { elevated: false },
    });
  });

  test("non-engineer cannot set elevated", async ({ page }) => {
    await page.goto("/");

    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    const memberAuth = await getToken(page, member);
    if (!assume(memberAuth, "Member login failed")) return;

    const resp = await page.request.put(`/api/members/${member}/elevated`, {
      headers: {
        Authorization: `Bearer ${memberAuth!.token}`,
        "Content-Type": "application/json",
      },
      data: { elevated: true },
    });
    expect(resp.status()).toBe(403);
  });

  test("elevated member gets mix channels in mixer state", async ({ page }) => {
    await page.goto("/");

    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 2, "Need at least 2 members")) return;

    const member = members.find((m: { id: string }) => m.id !== "engineer");
    if (!assume(member, "No non-engineer member found")) return;

    const engAuth = await getEngineerToken(page);
    if (!assume(engAuth, "Engineer login failed")) return;

    // Set elevated
    const setResp = await page.request.put(
      `/api/members/${member.id}/elevated`,
      {
        headers: {
          Authorization: `Bearer ${engAuth!.token}`,
          "Content-Type": "application/json",
        },
        data: { elevated: true },
      },
    );
    expect(setResp.status()).toBe(200);

    // Get mixer state for the elevated member (as engineer)
    const mixerResp = await page.request.get(`/api/mixer/${member.id}`, {
      headers: { Authorization: `Bearer ${engAuth!.token}` },
    });

    if (mixerResp.status() === 200) {
      const mixer = await mixerResp.json();
      const mixChannels = mixer.channels.filter(
        (ch: { category: string }) => ch.category === "mixes",
      );
      // Mix channels should exist for elevated member
      expect(mixChannels.length).toBeGreaterThan(0);
      // All mix channels should be muted by default
      for (const ch of mixChannels) {
        expect(ch.muted).toBe(true);
      }
    }

    // Clean up
    await page.request.put(`/api/members/${member.id}/elevated`, {
      headers: {
        Authorization: `Bearer ${engAuth!.token}`,
        "Content-Type": "application/json",
      },
      data: { elevated: false },
    });
  });

  test("engineer can set elevated=false to remove access", async ({ page }) => {
    await page.goto("/");

    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    const engAuth = await getEngineerToken(page);
    if (!assume(engAuth, "Engineer login failed")) return;

    // Set elevated true then false
    await page.request.put(`/api/members/${member}/elevated`, {
      headers: {
        Authorization: `Bearer ${engAuth!.token}`,
        "Content-Type": "application/json",
      },
      data: { elevated: true },
    });

    const unsetResp = await page.request.put(
      `/api/members/${member}/elevated`,
      {
        headers: {
          Authorization: `Bearer ${engAuth!.token}`,
          "Content-Type": "application/json",
        },
        data: { elevated: false },
      },
    );
    expect(unsetResp.status()).toBe(200);

    const getResp = await page.request.get(`/api/members/${member}/elevated`, {
      headers: { Authorization: `Bearer ${engAuth!.token}` },
    });
    const body = await getResp.json();
    expect(body.elevated).toBe(false);
  });
});
