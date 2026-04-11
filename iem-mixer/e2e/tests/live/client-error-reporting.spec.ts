import { test, expect } from "@playwright/test";
import { execFileSync } from "child_process";

/**
 * Post-deploy E2E for POST /api/client-error (#153).
 *
 * Posts a marker panic message from the browser, asserts the server returns
 * 204, then SSHs to iem.lan and greps the rolling tracing-appender log file
 * for the marker. Proves the endpoint is wired, the body limit works, and
 * the structured log line actually reaches the on-disk log file.
 *
 * Requires: SSH key for newlevel@iem.lan configured on the runner (already
 * set up by the deploy workflow).
 *
 * Log path (verified 2026-04-11): iem-mixer uses tracing_appender::rolling::daily()
 * in iem-mixer/src-tauri/src/lib.rs. On Windows, dirs::data_dir() returns
 * %APPDATA%, so logs live at:
 *   C:\Users\newlevel\AppData\Roaming\iem-mixer\logs\iem-mixer.log.YYYY-MM-DD
 */
test.describe("Client error reporting (#153)", () => {
  test("marker panic report reaches the rolling log file", async ({
    page,
  }) => {
    const marker = `e2e-marker-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

    // Navigate to the base URL — no auth needed, the endpoint is public.
    await page.goto("/");

    // POST from the browser context so we exercise CORS and the real path.
    const status = await page.evaluate(async (m) => {
      const res = await fetch("/api/client-error", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          panic_message: m,
          version: "1.142.0",
          git_hash: "e2e",
          url: "/e2e-test",
          user_agent: navigator.userAgent,
          location: "e2e:1:1",
        }),
      });
      return res.status;
    }, marker);

    expect(status).toBe(204);

    // tracing_appender::non_blocking buffers writes — give it a moment
    // to flush before we go looking in the file.
    await page.waitForTimeout(2000);

    // This test runs on the self-hosted runner which lives ON iem.lan, so
    // we read the log file directly from the local filesystem — no SSH
    // needed (the runner cannot SSH back to itself). Absolute path is used
    // because the runner process may not run as `newlevel`, which would
    // make $env:APPDATA point at a different user's Roaming folder. The
    // Tauri app always writes to newlevel's APPDATA (dirs::data_dir()
    // joined with "iem-mixer/logs/iem-mixer.log" + daily rolling suffix).
    const logGlob =
      "C:\\Users\\newlevel\\AppData\\Roaming\\iem-mixer\\logs\\iem-mixer.log.*";
    const psCommand =
      '$files = Get-ChildItem -Path "' +
      logGlob +
      '" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 2;' +
      "if ($files) { $files | ForEach-Object { Get-Content $_.FullName -Tail 500 } | Select-String '" +
      marker +
      "' }";

    let found = "";
    try {
      found = execFileSync(
        "powershell",
        ["-NoProfile", "-Command", psCommand],
        { timeout: 20_000 },
      ).toString();
    } catch (err) {
      throw new Error(
        `Local log grep for marker '${marker}' failed: ${(err as Error).message}`,
      );
    }

    expect(found).toContain(marker);
    expect(found.toLowerCase()).toContain("client_error");
  });

  test("oversize body is rejected with 413", async ({ page }) => {
    await page.goto("/");

    const status = await page.evaluate(async () => {
      // 12 KB body — exceeds the 10 KB route-level DefaultBodyLimit.
      const big = "x".repeat(12 * 1024);
      const res = await fetch("/api/client-error", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ panic_message: big }),
      });
      return res.status;
    });

    expect(status).toBe(413);
  });
});
