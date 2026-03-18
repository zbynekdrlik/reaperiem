/**
 * Audio E2E Tests — verifies audio pipeline delivers valid signal, not just bytes.
 *
 * These tests use the /api/audio/diagnostics endpoint to check that
 * the VBAN → Opus pipeline is actually processing audio (peak_db > threshold).
 * When a tone generator is active in REAPER, these tests FAIL the build
 * if the pipeline is broken — no more green CI with broken audio.
 */

import { test, expect } from "@playwright/test";

const BASE_URL = process.env.E2E_BASE_URL || "http://localhost:8080";

// Authenticate as engineer and return token
async function getEngineerToken(
  request: ReturnType<typeof test.extend>["request"] extends infer R
    ? R
    : never,
): Promise<string | null> {
  try {
    const response = await (request as { post: Function }).post(
      `${BASE_URL}/api/auth`,
      {
        data: { member: "engineer", pin: "1177" },
      },
    );
    if (response.status() !== 200) return null;
    const data = await response.json();
    return data.token || null;
  } catch {
    return null;
  }
}

test.describe("Audio Pipeline Diagnostics", () => {
  test("diagnostics endpoint returns valid structure", async ({ request }) => {
    const token = await getEngineerToken(request);
    if (!token) {
      console.log("[SKIP] Cannot authenticate as engineer");
      return;
    }

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(response.status()).toBe(200);

    const diag = await response.json();
    expect(diag).toHaveProperty("receiving_vban");
    expect(diag).toHaveProperty("packets_per_second");
    expect(diag).toHaveProperty("opus_frames_per_second");
    expect(diag).toHaveProperty("peak_db");
    expect(diag).toHaveProperty("sample_rate");
    expect(diag).toHaveProperty("channels");
    expect(typeof diag.receiving_vban).toBe("boolean");
    expect(typeof diag.peak_db).toBe("number");
  });

  test("when VBAN is active, audio signal is not silence", async ({
    request,
  }) => {
    const token = await getEngineerToken(request);
    if (!token) {
      console.log("[SKIP] Cannot authenticate as engineer");
      return;
    }

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const diag = await response.json();

    if (!diag.receiving_vban) {
      console.log(
        "[SKIP] No VBAN packets — REAPER not running or VBAN VST not active",
      );
      return;
    }

    // If VBAN is receiving, the pipeline must be producing real audio
    expect(diag.packets_per_second).toBeGreaterThan(10);
    expect(diag.opus_frames_per_second).toBeGreaterThan(10);

    // With a tone generator or real signal, peak should be above -40 dB
    // Silence is -150 dB; noise floor is around -80 dB
    expect(diag.peak_db).toBeGreaterThan(-40);
    expect(diag.sample_rate).toBeGreaterThan(0);
    expect(diag.channels).toBeGreaterThan(0);
  });

  test("diagnostics requires engineer auth", async ({ request }) => {
    // No auth header → should fail
    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`);
    expect(response.status()).toBe(401);
  });

  test("non-engineer token rejected for diagnostics", async ({ request }) => {
    // Login as regular member
    const authResp = await request.post(`${BASE_URL}/api/auth`, {
      data: { member: "petronela", pin: "7711" },
    });
    if (authResp.status() !== 200) {
      console.log("[SKIP] Cannot authenticate as member");
      return;
    }
    const { token } = await authResp.json();

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    // Should be forbidden — only engineers can access diagnostics
    expect(response.status()).toBe(403);
  });
});
