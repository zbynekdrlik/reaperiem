import { defineConfig, devices } from "@playwright/test";
import * as path from "path";

const FIXTURE =
  path.resolve(__dirname, "./tests/fixtures/talkback-1k-tone.wav");

// Chromium launch args shared across all live tests.
// - unsafely-treat-insecure-origin-as-secure: grants WebCodecs to plain-http
//   LAN origins like http://10.77.9.231 and http://localhost.
// - fake-ui / fake-device / use-file-for-fake-audio-capture: feed a known
//   1 kHz tone into getUserMedia for the talkback-quality spec. Other live
//   tests are unaffected because they do not call getUserMedia.
const BASE_URL = process.env.E2E_BASE_URL || "http://localhost";
const SECURE_ORIGINS = [
  BASE_URL,
  "http://10.77.9.231",
  "http://localhost",
  "http://localhost:80",
].join(",");

export default defineConfig({
  testDir: "./tests/live",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: 1,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
    permissions: ["microphone"],
    launchOptions: {
      args: [
        `--unsafely-treat-insecure-origin-as-secure=${SECURE_ORIGINS}`,
        "--use-fake-ui-for-media-stream",
        "--use-fake-device-for-media-stream",
        `--use-file-for-fake-audio-capture=${FIXTURE}`,
      ],
    },
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
