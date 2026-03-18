/**
 * Audio Pipeline E2E Test
 *
 * Tests the full server-side audio pipeline:
 *   Synthetic VBAN UDP → parse → resample → Opus encode → WebSocket binary frames
 *
 * This test does NOT require REAPER or a browser audio player.
 * It sends synthetic VBAN UDP packets and verifies Opus frames arrive on the WebSocket.
 */

import { test, expect } from "@playwright/test";
import * as dgram from "dgram";
import WebSocket from "ws";

const BASE_URL = process.env.E2E_BASE_URL || "http://localhost:8080";
const VBAN_PORT = 6980;

// VBAN protocol constants
const VBAN_MAGIC = Buffer.from([0x56, 0x42, 0x41, 0x4e]); // "VBAN" in LE (0x4E414256)
const VBAN_HEADER_SIZE = 28;

// VBAN sample rate table (index 4 = 96000 Hz)
const VBAN_SR_INDEX_96000 = 4;

/** Build a valid VBAN UDP packet with a 440Hz sine tone (INT16 format) */
function buildVBANPacket(
  sampleOffset: number,
  channels = 2,
  sampleRate = 96000,
  samplesPerCh = 256,
): Buffer {
  const audioBytes = samplesPerCh * channels * 2; // INT16 = 2 bytes per sample
  const buf = Buffer.alloc(VBAN_HEADER_SIZE + audioBytes);
  let offset = 0;

  // Magic "VBAN" (LE)
  VBAN_MAGIC.copy(buf, offset);
  offset += 4;

  // format_SR: sample rate index | protocol audio (0x00)
  const srIndex =
    sampleRate === 96000
      ? VBAN_SR_INDEX_96000
      : sampleRate === 48000
        ? 3
        : VBAN_SR_INDEX_96000;
  buf.writeUInt8(srIndex, offset);
  offset += 1;

  // format_nbs: samples per frame - 1
  buf.writeUInt8(samplesPerCh - 1, offset);
  offset += 1;

  // format_nbc: channels - 1
  buf.writeUInt8(channels - 1, offset);
  offset += 1;

  // format_bit: INT16 (1) | codec PCM (0x00)
  buf.writeUInt8(1, offset);
  offset += 1;

  // Stream name: "engineer" null-padded to 16 bytes
  buf.write("engineer", offset, "ascii");
  offset += 16; // rest is zero-filled by Buffer.alloc

  // Frame counter (u32 LE)
  buf.writeUInt32LE(Math.floor(sampleOffset / samplesPerCh), offset);
  offset += 4;

  // Audio: interleaved INT16 sine wave (440Hz)
  for (let i = 0; i < samplesPerCh; i++) {
    const t = (sampleOffset + i) / sampleRate;
    const val = 0.3 * Math.sin(2 * Math.PI * 440 * t);
    const int16Val = Math.round(val * 32767);
    for (let ch = 0; ch < channels; ch++) {
      buf.writeInt16LE(int16Val, offset);
      offset += 2;
    }
  }

  return buf;
}

/** Send VBAN UDP packets for a given duration */
function startUdpSender(durationMs: number): {
  stop: () => void;
  packetsSent: () => number;
} {
  const socket = dgram.createSocket("udp4");
  let sent = 0;
  let sampleOffset = 0;
  const samplesPerPacket = 256;
  const intervalMs = (samplesPerPacket / 96000) * 1000; // ~2.67ms

  const interval = setInterval(() => {
    const packet = buildVBANPacket(sampleOffset);
    socket.send(packet, VBAN_PORT, "127.0.0.1");
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

test.describe("Audio Pipeline (VBAN UDP → Opus → WebSocket)", () => {
  test("synthetic VBAN UDP packets produce Opus frames on WebSocket", async () => {
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
    const frameSizes: number[] = [];
    const tocBytes: number[] = [];

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
          frameSizes.push(data.length);
          if (data.length > 0) {
            tocBytes.push(data[0]);
          }
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

    // Assertions
    expect(binaryFrames).toBeGreaterThanOrEqual(5);
    expect(gotListening).toBe(true);
    expect(gotNoSource).toBe(false);

    // Validate Opus frame structure
    for (const size of frameSizes) {
      // Opus frames for 20ms stereo @ 64kbps are typically 80-200 bytes
      // Minimum valid Opus packet is 1 byte (silence), max ~1275 bytes
      expect(size).toBeGreaterThan(0);
      expect(size).toBeLessThan(1300);
    }

    // Validate Opus TOC byte structure
    // TOC byte encodes: config (5 bits) | stereo flag (1 bit) | code (2 bits)
    // For stereo, bit 2 (0x04) should be set
    for (const toc of tocBytes) {
      const stereoFlag = (toc >> 2) & 1;
      expect(stereoFlag).toBe(1); // must be stereo
    }

    console.log(
      `PASS: Received ${binaryFrames} valid Opus frames (sizes: ${frameSizes.slice(0, 5).join(",")}...) from ${sender.packetsSent()} VBAN UDP packets`,
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
