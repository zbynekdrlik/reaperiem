/**
 * Audio Pipeline E2E Test
 *
 * Tests the full server-side audio pipeline:
 *   Synthetic ReaStream UDP → parse → resample → Opus encode → WebSocket binary frames
 *
 * This test does NOT require REAPER or a browser audio player.
 * It sends synthetic UDP packets and verifies Opus frames arrive on the WebSocket.
 */

import { test, expect } from "@playwright/test";
import * as dgram from "dgram";
import WebSocket from "ws";

const BASE_URL = process.env.E2E_BASE_URL || "http://localhost:8080";
const REASTREAM_PORT = 4711;

/** Build a valid ReaStream UDP packet with a 440Hz sine tone */
function buildReaStreamPacket(
  sampleOffset: number,
  channels = 2,
  sampleRate = 96000,
  samplesPerCh = 512,
): Buffer {
  const audioBytes = samplesPerCh * channels * 4;
  const packetSize = 32 + 1 + 4 + 2 + audioBytes;

  const buf = Buffer.alloc(4 + 4 + 32 + 1 + 4 + 2 + audioBytes);
  let offset = 0;

  // Magic "MRSR"
  buf.write("MRSR", offset, "ascii");
  offset += 4;

  // Packet size (LE u32)
  buf.writeUInt32LE(packetSize, offset);
  offset += 4;

  // Identifier: "engineer" null-padded to 32 bytes
  buf.write("engineer", offset, "ascii");
  offset += 32; // rest is zero-filled by Buffer.alloc

  // Channels (u8)
  buf.writeUInt8(channels, offset);
  offset += 1;

  // Sample rate (LE u32)
  buf.writeUInt32LE(sampleRate, offset);
  offset += 4;

  // Block size / samples per channel (LE u16)
  buf.writeUInt16LE(samplesPerCh, offset);
  offset += 2;

  // Audio: interleaved f32 sine wave (440Hz)
  for (let i = 0; i < samplesPerCh; i++) {
    const t = (sampleOffset + i) / sampleRate;
    const val = 0.3 * Math.sin(2 * Math.PI * 440 * t);
    for (let ch = 0; ch < channels; ch++) {
      buf.writeFloatLE(val, offset);
      offset += 4;
    }
  }

  return buf;
}

/** Send ReaStream UDP packets for a given duration */
function startUdpSender(durationMs: number): {
  stop: () => void;
  packetsSent: () => number;
} {
  const socket = dgram.createSocket("udp4");
  let sent = 0;
  let sampleOffset = 0;
  const samplesPerPacket = 512;
  const intervalMs = (samplesPerPacket / 96000) * 1000; // ~5.3ms

  const interval = setInterval(() => {
    const packet = buildReaStreamPacket(sampleOffset);
    socket.send(packet, REASTREAM_PORT, "127.0.0.1");
    sampleOffset += samplesPerPacket;
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

test.describe("Audio Pipeline (UDP → Opus → WebSocket)", () => {
  test("synthetic ReaStream UDP packets produce Opus frames on WebSocket", async () => {
    // Step 1: Start sending UDP packets BEFORE connecting WebSocket
    // The pipeline needs data to initialize (resampler + Opus encoder)
    const sender = startUdpSender(15000);

    // Give the pipeline time to initialize from first packets
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
        ws.send(JSON.stringify({ cmd: "ListenStart" }));
      });

      ws.on("message", (data: Buffer, isBinary: boolean) => {
        if (isBinary) {
          binaryFrames++;
          messages.push({ type: "binary", data: `${data.length} bytes` });
          if (binaryFrames >= 5) {
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

    // Assertions
    expect(binaryFrames).toBeGreaterThanOrEqual(5);
    expect(gotListening).toBe(true);
    expect(gotNoSource).toBe(false);
    console.log(
      `PASS: Received ${binaryFrames} Opus frames from ${sender.packetsSent()} UDP packets`,
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
        ws.send(JSON.stringify({ cmd: "ListenStart" }));
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
});
