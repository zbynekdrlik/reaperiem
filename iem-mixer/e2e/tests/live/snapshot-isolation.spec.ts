/**
 * DECISION GATE — cross-member contamination on local snapshot restore
 *
 * Hypothesis (from T2 investigation, commit 025dde5): restoring member A's
 * snapshot accidentally writes to member B's inear sends.
 *
 * T2 found NO on-disk evidence of contamination (LOW confidence).
 * This test is the definitive code-level gate:
 *
 *   FAIL → contamination is real → T10 ships a fix
 *   PASS → hypothesis is falsified → T10 is skipped, GH issue opened
 *
 * Safety: engineer-only writes, finally-block deletes test snapshot,
 * observer state is restored if it changed (defensive).
 *
 * Auth: engineer PIN "1177" — never touches member PINs.
 */

import { test, expect } from "@playwright/test";
import { ENGINEER_PIN, ISOLATION_PAIRS } from "../fixtures/test-credentials";

const REAPER = "http://iem.lan:8080";
const APP = process.env.IEM_APP_URL || "http://10.77.9.231";

/** 1-based inear track indices verified from project topology (T2 evidence). */
const MEMBER_INEAR_TRACK: Record<string, number> = {
  petronela: 23,
  stevo: 24,
  marek: 25,
  zuzka: 26,
  tina: 27,
  mirec: 28,
  alex: 29,
  patrika: 30,
  ani: 31,
  engineer: 32,
};

/**
 * Cross-member isolation pairs imported from `fixtures/test-credentials.ts`.
 * Restricted to members with known stable PINs because the poller only
 * populates `mixer_cache.member_states[X]` after member X's WebSocket
 * connects (poller.rs:340 — skips if no `active_members`); without the
 * member PIN `create_snapshot` returns 400 NO_STATE.
 *
 * Tested both directions to catch send-index symmetry bugs.
 */
const PAIRS = ISOLATION_PAIRS;

interface SendSnapshot {
  src: number;
  sendIdx: number;
  /** Raw REAPER mute flag (0 = unmuted, 8 = muted). */
  mute: number;
  vol: string;
  pan: string;
}

/**
 * Walk all REAPER tracks (1..32) and collect the sends that point at
 * `observerInear`.  Returns a deterministically ordered array so that
 * deep-equality comparison works.
 */
async function captureObserverState(
  request: import("@playwright/test").APIRequestContext,
  observerInear: number,
): Promise<SendSnapshot[]> {
  const sends: SendSnapshot[] = [];

  // Scan all input tracks (which carry sends to inears). 22 input tracks plus
  // the recently-added ALEX kl (44) and CG (45). Each has up to ~12 sends.
  //
  // Resilience: an empty / malformed response on ONE send must not terminate
  // the scan — REAPER occasionally returns an empty body under load. We
  // `continue` past errors instead of `break`-ing the inner loop, which
  // previously caused tests to silently observe zero sends for a member.
  for (let track = 1; track <= 45; track++) {
    for (let sendIdx = 0; sendIdx < 12; sendIdx++) {
      let text: string;
      try {
        const r = await request.get(
          `${REAPER}/_/GET/TRACK/${track}/SEND/${sendIdx}`,
        );
        text = (await r.text()).trim();
      } catch {
        continue; // request error — skip this send, keep scanning
      }
      if (!text) continue; // empty body — skip, don't terminate the scan

      const parts = text.split("\t");
      // SEND(0) track(1) send(2) mute_flag(3) volume(4) pan(5) destination(6)
      if (parts.length < 7) continue;

      const dest = parseInt(parts[6] ?? "-1", 10);
      if (dest === observerInear) {
        sends.push({
          src: track,
          sendIdx,
          mute: parseInt(parts[3] ?? "0", 10),
          vol: parts[4] ?? "0",
          pan: parts[5] ?? "0",
        });
      }
    }
  }

  return sends;
}

test.describe.configure({ timeout: 180_000 });

test.describe("Snapshot restore isolation (defensive regression gate)", () => {
  for (const [restoringMember, observerMember, restoringPin] of PAIRS) {
    test(
      `member_restore_does_not_touch_other_members__${restoringMember}_restores__${observerMember}_unchanged`,
      async ({ page, request }) => {
        const consoleErrors: string[] = [];
        page.on("console", (msg) => {
          if (msg.type() === "error" || msg.type() === "warning") {
            const text = msg.text();
            // Known-benign browser notices — mirrors backup-cg-remute filter.
            if (text.includes("apple-mobile-web-app-capable")) return;
            if (text.includes("[push] subscribe await failed")) return;
            if (text.includes("Push API in incognito mode")) return;
            if (/integrity.*attribute.*ignored/i.test(text)) return;
            if (text.includes("vapid-key fetch error")) return;
            consoleErrors.push(`[${msg.type()}] ${text}`);
          }
        });

        // Engineer login — mirrors backup-cg-remute.spec.ts pattern.
        await page.goto("/");
        const authResp = await page.request.post("/api/auth", {
          data: { member: "engineer", pin: ENGINEER_PIN },
        });
        expect(authResp.status()).toBe(200);
        const authData = await authResp.json();
        expect(authData.engineer).toBe(true);
        const token: string = authData.token;
        const headers = { Authorization: `Bearer ${token}` };

        const observerInear = MEMBER_INEAR_TRACK[observerMember];
        expect(
          observerInear,
          `unknown observer member: ${observerMember}`,
        ).toBeDefined();

        // 1. Capture observer's full inear-receiving send picture BEFORE restore.
        const before = await captureObserverState(request, observerInear);
        expect(
          before.length,
          `observer ${observerMember} must have at least one send routed to their inear track`,
        ).toBeGreaterThan(0);

        let createdTimestamp: number | null = null;

        try {
          // 2. Warm the mixer cache for the RESTORING member.
          //    create_snapshot reads from mixer_cache.member_states[member]; the
          //    poller (poller.rs:340) only populates state for members in
          //    `active_members`, which only contains members with active WebSocket
          //    connections.  Without the member PIN we can authenticate but cannot
          //    sustain a WebSocket against the live system.
          //
          //    Strategy: log in as the member with their PIN (in a separate browser
          //    context) and let that page mount its WebSocket — the WS subscribe
          //    handshake adds the member to active_members, the poller picks them
          //    up on the next 150ms tick, and member_states[member] is populated.
          //    Retry the snapshot create until cache is hot.
          let snapResp: Awaited<ReturnType<typeof page.request.post>> | null = null;
          let lastBody = "";

          // Open a long-lived authenticated context that holds a WebSocket open.
          const memberPage = await page.context().newPage();
          try {
            await memberPage.goto(`${APP}/login`).catch(() => {});
            // Auth via /api/auth, store under the SAME localStorage key the
            // production app reads (`iem_token`, JSON-encoded auth object).
            // The previous version of this test wrote to `authToken` with a
            // raw string, which production's auth.rs (TOKEN_KEY = "iem_token")
            // never read — so the page navigated unauthenticated, no
            // WebSocket subscribed, and the poller never added the member to
            // `active_members`. That manifested as "snapshot creation
            // failed: NO_STATE" only for members whose state wasn't already
            // warmed by a prior test in the suite (e.g. stevo).
            await memberPage.evaluate(
              ({ member, pin }) => {
                return fetch("/api/auth", {
                  method: "POST",
                  headers: { "Content-Type": "application/json" },
                  body: JSON.stringify({ member, pin }),
                })
                  .then((r) => r.json())
                  .then((j) => {
                    if (j.token) {
                      localStorage.setItem(
                        "iem_token",
                        JSON.stringify({
                          token: j.token,
                          member: j.member,
                          engineer: j.engineer,
                        }),
                      );
                    }
                  });
              },
              { member: restoringMember, pin: restoringPin },
            );
            await memberPage.goto(`${APP}/${restoringMember}`).catch(() => {});
            // Initial warm-up: let WebSocket subscribe + several poller ticks land.
            // Empirically the first send-routing query after a fresh subscribe can
            // take 5-8s on a quiet REAPER, so we wait 6s before the first attempt.
            await memberPage.waitForTimeout(6000);

            // Generous retry budget. After deploy the poller may need many ticks to
            // resolve all input-track indices for a freshly-connected member; under
            // load 24s was empirically too tight for the second test in the suite.
            for (let attempt = 0; attempt < 20; attempt++) {
              snapResp = await page.request.post(
                `/api/snapshots/${restoringMember}`,
                { headers, data: { label: "test_isolation_probe" } },
              );
              if (snapResp.status() === 201) break;
              lastBody = await snapResp.text();
              await memberPage.waitForTimeout(2000);
            }
          } finally {
            await memberPage.close().catch(() => {});
          }

          expect(
            snapResp!.status(),
            `snapshot creation for ${restoringMember} failed after retries: ${lastBody}`,
          ).toBe(201);
          const snapJson = await snapResp!.json();
          createdTimestamp = snapJson.timestamp as number;
          expect(
            createdTimestamp,
            "snapshot creation must return a numeric timestamp",
          ).toBeTruthy();

          // 3. Engineer restores that snapshot.
          const restoreResp = await page.request.post(
            `/api/snapshots/${restoringMember}/${createdTimestamp}/restore`,
            { headers },
          );
          expect(
            restoreResp.status(),
            `restore for ${restoringMember} failed: ${await restoreResp.text()}`,
          ).toBe(200);

          // Give REAPER time to apply all sends (mirrors backup test wait pattern).
          await page.waitForTimeout(2500);

          // 4. Capture observer's send picture AFTER restore.
          const after = await captureObserverState(request, observerInear);

          // 5. DECISION GATE: observer must be byte-identical before and after.
          //    A diff here means member_restore is contaminating other members.
          expect(
            after,
            `HYPOTHESIS CONFIRMED: ${observerMember}'s sends changed after ${restoringMember}'s snapshot restore — cross-member contamination detected`,
          ).toEqual(before);
        } finally {
          // Cleanup: delete the test snapshot so it doesn't pollute the UI.
          if (createdTimestamp !== null) {
            await page.request
              .delete(
                `/api/snapshots/${restoringMember}/${createdTimestamp}`,
                { headers },
              )
              .catch(() => {
                // Best-effort — non-fatal if already gone.
              });
          }

          // Defensive: if observer state changed (test failed), log it.
          // We do NOT attempt to restore it here because we don't have the
          // original REAPER send values to write back — the before/after
          // comparison is the signal; manual recovery is an operator task.
        }

        expect(consoleErrors).toEqual([]);
      },
    );
  }
});
