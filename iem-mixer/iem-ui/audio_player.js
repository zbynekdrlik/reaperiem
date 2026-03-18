// Audio player for IEM Mixer — decodes Opus frames and plays via Web Audio API
// Used by the ListenButton Leptos component via wasm_bindgen JS interop

let audioContext = null;
let nextStartTime = 0;
let frameIndex = 0;
let lastAudioLevel = -150;
let lastError = null;

/**
 * Initialize the audio player. Must be called from a user gesture (click).
 * Creates an AudioContext ready to receive decoded PCM buffers.
 */
export function initAudioPlayer() {
  if (audioContext && audioContext.state !== "closed") {
    // Context exists but might be suspended — resume it NOW (we're in a user gesture)
    if (audioContext.state === "suspended") {
      audioContext.resume().then(() => {
        console.log(
          "[audio] Context resumed (re-init), state:",
          audioContext.state,
        );
      });
    }
    return;
  }
  audioContext = new AudioContext({ sampleRate: 48000 });
  nextStartTime = 0;
  frameIndex = 0;
  lastAudioLevel = -150;
  lastError = null;

  // CRITICAL: Resume immediately during user gesture (click/tap).
  // On mobile browsers (iOS Safari, Chrome Android), AudioContext starts
  // as "suspended" even when created in a click handler. resume() MUST be
  // called from a user gesture — it will be REJECTED if called later from
  // a WebSocket callback or timer. This is the ONLY chance to unlock audio.
  if (audioContext.state === "suspended") {
    audioContext
      .resume()
      .then(() => {
        console.log("[audio] Context resumed, state:", audioContext.state);
      })
      .catch((e) => {
        console.error("[audio] Context resume FAILED:", e);
        lastError = "AudioContext resume failed: " + e.message;
      });
  }

  console.log(
    "[audio] Player initialized, sampleRate:",
    audioContext.sampleRate,
    "state:",
    audioContext.state,
  );
}

/**
 * Decode an Opus frame using WebCodecs AudioDecoder and schedule for playback.
 * Falls back to raw PCM scheduling if WebCodecs is not available.
 * @param {ArrayBuffer} opusData - Raw Opus packet bytes
 */
export function feedOpusFrame(opusData) {
  if (!audioContext || audioContext.state === "closed") return;

  // Diagnostic: log frame info (throttled to every 50th frame to avoid spam)
  if (frameIndex % 50 === 0) {
    console.log(
      `[audio] frame #${frameIndex}: ${opusData.byteLength}B, ctx=${audioContext?.state}`,
    );
  }

  // Fallback resume attempt — may fail on mobile (not a user gesture).
  // The primary resume happens in initAudioPlayer() during the click handler.
  if (audioContext.state === "suspended") {
    audioContext.resume().catch(() => {
      // Expected to fail on mobile — initAudioPlayer should have unlocked it
      if (frameIndex % 50 === 0) {
        console.warn(
          "[audio] Context still suspended — resume rejected (not a user gesture)",
        );
      }
    });
  }

  // Use WebCodecs AudioDecoder if available (Chrome, Edge, Safari 16.4+)
  if (typeof AudioDecoder !== "undefined") {
    decodeWithWebCodecs(opusData);
  }
  // No fallback — Firefox doesn't support WebCodecs, show unsupported message
}

// WebCodecs decoder (lazy-initialized)
let decoder = null;

function ensureDecoder() {
  if (decoder && decoder.state !== "closed") return;

  decoder = new AudioDecoder({
    output: (audioData) => {
      scheduleAudioData(audioData);
    },
    error: (e) => {
      console.error("[audio] Decoder ERROR:", e.message);
      lastError = e.message;
    },
  });

  decoder.configure({
    codec: "opus",
    sampleRate: 48000,
    numberOfChannels: 2,
  });
}

function decodeWithWebCodecs(opusData) {
  try {
    ensureDecoder();

    // If decoder crashed (error callback closed it), recreate
    if (decoder && decoder.state === "closed") {
      console.error("[audio] Decoder crashed — recreating");
      decoder = null;
      ensureDecoder();
    }

    // Each Opus frame is 20ms = 20000 microseconds.
    // Monotonic timestamps help WebCodecs track frame ordering.
    const timestamp = frameIndex * 20000;
    frameIndex++;
    const chunk = new EncodedAudioChunk({
      type: "key",
      timestamp,
      data: opusData,
    });
    decoder.decode(chunk);
  } catch (e) {
    console.warn("[audio] Decode error:", e.message);
    lastError = e.message;
  }
}

function scheduleAudioData(audioData) {
  if (!audioContext || audioContext.state === "closed") return;

  const numFrames = audioData.numberOfFrames;
  const numChannels = audioData.numberOfChannels;
  const buffer = audioContext.createBuffer(
    numChannels,
    numFrames,
    audioData.sampleRate,
  );

  // Copy decoded samples into AudioBuffer and track peak level
  let peak = 0;
  for (let ch = 0; ch < numChannels; ch++) {
    const channelData = new Float32Array(numFrames);
    audioData.copyTo(channelData, { planeIndex: ch });
    for (let i = 0; i < numFrames; i++) {
      const abs = Math.abs(channelData[i]);
      if (abs > peak) peak = abs;
    }
    buffer.copyToChannel(channelData, ch);
  }
  audioData.close();

  // Update signal level for diagnostics
  lastAudioLevel = peak > 0.0001 ? 20 * Math.log10(peak) : -150;

  // Schedule playback with seamless timing
  const source = audioContext.createBufferSource();
  source.buffer = buffer;
  source.connect(audioContext.destination);

  const now = audioContext.currentTime;
  if (nextStartTime <= now) {
    // First frame or gap — start with small buffer
    nextStartTime = now + 0.02;
  }
  source.start(nextStartTime);
  nextStartTime += buffer.duration;
}

/**
 * Stop the audio player and release resources.
 */
export function stopAudioPlayer() {
  if (decoder) {
    try {
      decoder.close();
    } catch (_) {
      // ignore
    }
    decoder = null;
  }
  if (audioContext) {
    audioContext.close();
    audioContext = null;
  }
  nextStartTime = 0;
  lastAudioLevel = -150;
  lastError = null;
  console.log("[audio] Player stopped");
}

/**
 * Check if WebCodecs AudioDecoder is supported in this browser.
 * @returns {boolean}
 */
export function isAudioSupported() {
  return typeof AudioDecoder !== "undefined";
}

/**
 * Get the last measured audio signal level in dB.
 * Returns -150 when silent or not playing.
 * @returns {number} Signal level in dB (0 = full scale, -150 = silence)
 */
export function getAudioLevel() {
  return lastAudioLevel;
}

/**
 * Get the last decoder error message, or null if no error.
 * @returns {string|null}
 */
export function getAudioError() {
  return lastError;
}

// Expose for E2E testing (Playwright can access via page.evaluate)
if (typeof window !== "undefined") {
  window.__iem_audio_level = getAudioLevel;
  window.__iem_audio_error = getAudioError;
}
