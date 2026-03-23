/**
 * Audio Pipeline E2E Test
 *
 * Tests the full server-side audio pipeline:
 *   Synthetic OIEM UDP → relay → WebSocket binary frames
 *
 * This test does NOT require REAPER or a browser audio player.
 * It sends synthetic OIEM UDP packets (pre-encoded Opus payloads) and verifies
 * they arrive on the WebSocket.
 *
 * The server is a pure relay — it validates the OIEM header and forwards the
 * Opus payload directly. No resampling or encoding happens server-side.
 *
 * OIEM packet format:
 *   Bytes 0-3: Magic "OIEM" (0x4F49454D)
 *   Bytes 4-5: Sequence number (uint16 LE, wrapping)
 *   Bytes 6-7: Payload size in bytes (uint16 LE)
 *   Bytes 8+:  Raw Opus frame
 */

import { test, expect } from "@playwright/test";
import * as dgram from "dgram";
import WebSocket from "ws";

const BASE_URL = process.env.E2E_BASE_URL || "http://localhost:8080";
const OIEM_PORT = 6980;

// OIEM protocol constants
const OIEM_MAGIC = Buffer.from([0x4f, 0x49, 0x45, 0x4d]); // "OIEM"
const OIEM_HEADER_SIZE = 8;

/** Build a valid OIEM UDP packet with synthetic payload */
function buildOIEMPacket(sequenceNumber: number, payloadSize = 200): Buffer {
  const buf = Buffer.alloc(OIEM_HEADER_SIZE + payloadSize);
  let offset = 0;

  // Magic "OIEM"
  OIEM_MAGIC.copy(buf, offset);
  offset += 4;

  // Sequence number (uint16 LE)
  buf.writeUInt16LE(sequenceNumber & 0xffff, offset);
  offset += 2;

  // Payload size (uint16 LE)
  buf.writeUInt16LE(payloadSize, offset);
  offset += 2;

  // Synthetic payload (non-zero data mimicking Opus frame)
  for (let i = 0; i < payloadSize; i++) {
    buf.writeUInt8((sequenceNumber + i) % 256, offset);
    offset += 1;
  }

  return buf;
}

/** Send OIEM UDP packets for a given duration */
function startUdpSender(durationMs: number): {
  stop: () => void;
  packetsSent: () => number;
} {
  const socket = dgram.createSocket("udp4");
  let sent = 0;
  // Send at 50 packets/sec (matching real 20ms Opus frame rate)
  const intervalMs = 20;

  const interval = setInterval(() => {
    const packet = buildOIEMPacket(sent);
    socket.send(packet, OIEM_PORT, "127.0.0.1");
    sent++;
  }, intervalMs);

  const timeout = setTimeout(() => {
    clearInterval(interval);
    socket.close();
  }, durationMs);

  return {
    stop: () => {
      clearInterval(interval);
      clearTimeout(timeout);
      try {
        socket.close();
      } catch {
        /* already closed */
      }
    },
    packetsSent: () => sent,
  };
}

/** Get an engineer JWT token */
async function getEngineerToken(): Promise<string> {
  const resp = await fetch(`${BASE_URL}/api/auth`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ member: "engineer", pin: "1177" }),
  });
  if (!resp.ok) {
    throw new Error(`Auth failed: ${resp.status} ${await resp.text()}`);
  }
  const data = await resp.json();
  return data.token;
}

test.describe("Audio Pipeline (OIEM UDP → WebSocket)", () => {
  test("synthetic OIEM UDP packets produce binary frames on WebSocket", async () => {
    // Step 1: Start sending OIEM packets BEFORE connecting WebSocket
    const sender = startUdpSender(15000);

    // Give the relay time to start receiving
    await new Promise((r) => setTimeout(r, 2000));

    // Step 2: Get engineer JWT token
    const token = await getEngineerToken();

    // Step 3: Connect to audio WebSocket
    const wsUrl = `${BASE_URL.replace("http", "ws")}/ws/audio?token=${token}`;
    const ws = new WebSocket(wsUrl);

    const messages: { type: "text" | "binary"; data: unknown }[] = [];
    let binaryFrames = 0;
    let gotListening = false;
    let gotNoSource = false;
    const frameSizes: number[] = [];

    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(
          new Error(
            `Timeout: got ${binaryFrames} binary frames, ${messages.length} messages total. ` +
              `listening=${gotListening}, no_source=${gotNoSource}, ` +
              `UDP packets sent=${sender.packetsSent()}`,
          ),
        );
      }, 12000);

      ws.on("open", () => {
        // Send ListenStart
        ws.send(JSON.stringify({ cmd: "ListenStart", member_id: "engineer" }));
      });

      ws.on("message", (data: Buffer, isBinary: boolean) => {
        if (isBinary) {
          binaryFrames++;
          frameSizes.push(data.length);
          messages.push({ type: "binary", data: `${data.length} bytes` });
          if (binaryFrames >= 10) {
            clearTimeout(timeout);
            resolve();
          }
        } else {
          const text = data.toString("utf-8");
          messages.push({ type: "text", data: text });
          try {
            const msg = JSON.parse(text);
            if (msg?.data?.status === "listening") gotListening = true;
            if (msg?.data?.status === "no_source") gotNoSource = true;
          } catch {
            /* not JSON */
          }
        }
      });

      ws.on("error", (err) => {
        clearTimeout(timeout);
        reject(new Error(`WebSocket error: ${err.message}`));
      });

      ws.on("close", () => {
        clearTimeout(timeout);
        if (binaryFrames < 5) {
          reject(
            new Error(
              `WebSocket closed early: ${binaryFrames} binary frames, ${messages.length} messages`,
            ),
          );
        }
      });
    });

    // Cleanup
    sender.stop();
    ws.close();

    // === Basic frame delivery assertions ===
    expect(binaryFrames).toBeGreaterThanOrEqual(5);
    expect(gotListening).toBe(true);
    expect(gotNoSource).toBe(false);

    // === Frame size validation ===
    // Server relays the Opus payload directly — should match our synthetic 200 bytes
    for (const size of frameSizes) {
      expect(size).toBe(200);
    }

    const avgSize = frameSizes.reduce((a, b) => a + b, 0) / frameSizes.length;
    console.log(
      `PASS: ${binaryFrames} frames relayed, avg=${avgSize.toFixed(0)}B`,
    );
  });

  test("audio WebSocket returns no_source without UDP packets", async () => {
    // NO UDP sender — should get no_source after 5s timeout

    const token = await getEngineerToken();
    const wsUrl = `${BASE_URL.replace("http", "ws")}/ws/audio?token=${token}`;
    const ws = new WebSocket(wsUrl);

    let gotNoSource = false;
    let gotListening = false;

    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error("Timeout waiting for no_source status"));
      }, 10000);

      ws.on("open", () => {
        ws.send(JSON.stringify({ cmd: "ListenStart", member_id: "engineer" }));
      });

      ws.on("message", (data: Buffer, isBinary: boolean) => {
        if (!isBinary) {
          try {
            const msg = JSON.parse(data.toString("utf-8"));
            if (msg?.data?.status === "listening") gotListening = true;
            if (msg?.data?.status === "no_source") {
              gotNoSource = true;
              clearTimeout(timeout);
              resolve();
            }
          } catch {
            /* not JSON */
          }
        }
      });

      ws.on("error", (err) => {
        clearTimeout(timeout);
        reject(new Error(`WebSocket error: ${err.message}`));
      });
    });

    ws.close();

    expect(gotListening).toBe(true);
    expect(gotNoSource).toBe(true);
  });

  test("audio WebSocket rejects non-engineer token", async () => {
    // Login as regular member
    const resp = await fetch(`${BASE_URL}/api/auth`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ member: "petronela", pin: "7711" }),
    });

    if (!resp.ok) {
      // Member might not exist in CI — skip gracefully
      console.log("[ASSUME SKIP] petronela member not available");
      return;
    }

    const data = await resp.json();
    const wsUrl = `${BASE_URL.replace("http", "ws")}/ws/audio?token=${data.token}`;
    const ws = new WebSocket(wsUrl);

    await new Promise<void>((resolve) => {
      ws.on("unexpected-response", (_req, res) => {
        // Should get 403 Forbidden
        expect(res.statusCode).toBe(403);
        resolve();
      });

      ws.on("open", () => {
        // Should NOT connect
        ws.close();
        throw new Error(
          "Non-engineer should not be able to connect to audio WS",
        );
      });

      // Timeout fallback
      setTimeout(() => resolve(), 5000);
    });
  });

  test("audio WebSocket rejects missing token", async () => {
    const wsUrl = `${BASE_URL.replace("http", "ws")}/ws/audio`;
    const ws = new WebSocket(wsUrl);

    await new Promise<void>((resolve) => {
      ws.on("unexpected-response", (_req, res) => {
        expect(res.statusCode).toBe(401);
        resolve();
      });

      ws.on("open", () => {
        ws.close();
        throw new Error("Missing token should not connect");
      });

      setTimeout(() => resolve(), 5000);
    });
  });

  test("diagnostics API reports signal when OIEM is active", async () => {
    // Start OIEM sender
    const sender = startUdpSender(10000);
    await new Promise((r) => setTimeout(r, 3000)); // Let pipeline accumulate data

    const token = await getEngineerToken();
    const resp = await fetch(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(resp.status).toBe(200);

    const diag = await resp.json();
    sender.stop();

    console.log(
      `Diagnostics: receiving=${diag.receiving_oiem}, peak=${diag.peak_db}dB, opus=${diag.opus_frames_per_second}/s`,
    );

    // Must be receiving OIEM
    expect(diag.receiving_oiem).toBe(true);

    // Peak dB is estimated from Opus frame size — synthetic 200-byte frames
    // map to roughly -13 dB with the server's estimation formula
    expect(diag.peak_db).toBeGreaterThan(-30);

    // Must be relaying Opus frames (50fps for 20ms frames)
    expect(diag.opus_frames_per_second).toBeGreaterThan(10);

    // No sequence gaps expected on localhost
    expect(diag.sequence_gaps).toBe(0);
  });
});
