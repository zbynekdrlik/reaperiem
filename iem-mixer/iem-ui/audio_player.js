// Audio player for IEM Mixer — decodes Opus frames and plays via Web Audio API
// Used by the ListenButton Leptos component via wasm_bindgen JS interop

let audioContext = null;
let gainNode = null;
let nextStartTime = 0;
let frameIndex = 0;
let lastAudioLevel = -150;
let lastError = null;
let pendingFrames = [];

// Jitter buffer state
const FRAME_DURATION_MS = 20;
const MIN_BUFFER_MS = 50;
const MAX_BUFFER_MS = 500;
const INITIAL_BUFFER_MS = 150;
let bufferDepthMs = INITIAL_BUFFER_MS;
let initialBuffering = true;
let frameQueue = [];
let lastFrameArrivalTime = 0;

// Dropout counter state
let dropoutCount = 0;
let totalFrames = 0;
let lastSourceEndTime = 0; // tracks when no frames arrive for extended periods

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
  gainNode = audioContext.createGain();
  gainNode.gain.value = 1.0;
  gainNode.connect(audioContext.destination);
  nextStartTime = 0;
  frameIndex = 0;
  lastAudioLevel = -150;
  lastError = null;

  // Reset jitter buffer
  bufferDepthMs = INITIAL_BUFFER_MS;
  initialBuffering = true;
  frameQueue = [];
  lastFrameArrivalTime = 0;

  // Reset dropout counters
  dropoutCount = 0;
  totalFrames = 0;
  lastSourceEndTime = 0;

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

  // When context transitions to "running", drain any buffered frames
  audioContext.onstatechange = () => {
    if (audioContext.state === "running" && pendingFrames.length > 0) {
      console.log(
        `[audio] Context running — draining ${pendingFrames.length} buffered frames`,
      );
      nextStartTime = 0;
      const frames = pendingFrames.splice(0);
      for (const frame of frames) {
        decodeWithWebCodecs(frame);
      }
    }
  };

  console.log(
    "[audio] Player initialized, sampleRate:",
    audioContext.sampleRate,
    "state:",
    audioContext.state,
    "jitter buffer:",
    bufferDepthMs + "ms",
  );
}

/**
 * Decode an Opus frame using WebCodecs AudioDecoder and schedule for playback.
 * Falls back to raw PCM scheduling if WebCodecs is not available.
 * @param {ArrayBuffer} opusData - Raw Opus packet bytes
 */
export function feedOpusFrame(opusData) {
  if (!audioContext || audioContext.state === "closed") return;

  const now = performance.now();
  totalFrames++;

  // Detect dropouts: gap > 60ms between frame arrivals (3x expected 20ms)
  if (lastFrameArrivalTime > 0) {
    const delta = now - lastFrameArrivalTime;
    if (delta > 60) {
      dropoutCount++;
      // Adapt buffer upward on jitter
      bufferDepthMs = Math.min(bufferDepthMs + 20, MAX_BUFFER_MS);
    } else {
      // Slowly shrink buffer when stable
      bufferDepthMs = Math.max(bufferDepthMs - 1, MIN_BUFFER_MS);
    }
  }
  lastFrameArrivalTime = now;

  // Diagnostic: log frame info (throttled to every 50th frame to avoid spam)
  if (frameIndex % 50 === 0) {
    console.log(
      `[audio] frame #${frameIndex}: ${opusData.byteLength}B, ctx=${audioContext?.state}, buf=${bufferDepthMs}ms, drops=${dropoutCount}/${totalFrames}`,
    );
  }

  // Buffer frames while context is not running (resume pending)
  if (audioContext.state !== "running") {
    if (pendingFrames.length < 50) {
      pendingFrames.push(new Uint8Array(opusData));
    }
    // Keep trying to resume — may succeed on some browsers without gesture
    audioContext.resume().catch(() => {});
    return;
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

  // Copy decoded samples into AudioBuffer and track peak level.
  // CRITICAL: Request f32-planar format explicitly. The decoder may output
  // interleaved (f32) where planeIndex:0 contains ALL channels interleaved,
  // making a Float32Array(numFrames) too small. Specifying format:"f32-planar"
  // tells the browser to give us per-channel planar data.
  let peak = 0;
  for (let ch = 0; ch < numChannels; ch++) {
    const opts = { planeIndex: ch, format: "f32-planar" };
    const byteSize = audioData.allocationSize(opts);
    const channelData = new Float32Array(byteSize / 4);
    audioData.copyTo(channelData, opts);
    for (let i = 0; i < channelData.length; i++) {
      const abs = Math.abs(channelData[i]);
      if (abs > peak) peak = abs;
    }
    buffer.copyToChannel(channelData, ch);
  }
  audioData.close();

  // Update signal level for diagnostics
  lastAudioLevel = peak > 0.0001 ? 20 * Math.log10(peak) : -150;

  // Schedule playback with jitter buffer
  const source = audioContext.createBufferSource();
  source.buffer = buffer;
  source.connect(gainNode || audioContext.destination);

  const now = audioContext.currentTime;
  if (nextStartTime <= now) {
    // First frame or gap — schedule ahead by buffer depth to absorb future jitter
    nextStartTime = now + bufferDepthMs / 1000;
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
  gainNode = null;
  if (audioContext) {
    audioContext.close();
    audioContext = null;
  }
  nextStartTime = 0;
  lastAudioLevel = -150;
  lastError = null;
  pendingFrames = [];
  frameQueue = [];
  initialBuffering = true;
  bufferDepthMs = INITIAL_BUFFER_MS;
  lastFrameArrivalTime = 0;
  dropoutCount = 0;
  totalFrames = 0;
  lastSourceEndTime = 0;
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

/**
 * Set the listen gain in dB. Range: -60 to +18.
 * @param {number} db - Gain in dB (0 = unity, +18 = max boost)
 */
export function setListenGain(db) {
  if (!gainNode) return;
  const linear = Math.pow(10, db / 20);
  gainNode.gain.setValueAtTime(linear, audioContext.currentTime);
}

/**
 * Get the current listen gain in dB.
 * @returns {number} Current gain in dB
 */
export function getListenGain() {
  if (!gainNode) return 0;
  const linear = gainNode.gain.value;
  return linear > 0.00001 ? 20 * Math.log10(linear) : -60;
}

/**
 * Get stream quality statistics.
 * @returns {{ dropouts: number, frames: number, bufferMs: number, quality: string }}
 */
export function getStreamStats() {
  if (!audioContext || totalFrames === 0) {
    return { dropouts: 0, frames: 0, bufferMs: 0, quality: "good" };
  }

  const dropoutRate = dropoutCount / Math.max(totalFrames, 1);
  const timeSinceLastFrame = performance.now() - lastFrameArrivalTime;

  let quality;
  if (timeSinceLastFrame > 2000) {
    quality = "poor";
  } else if (dropoutRate > 0.05) {
    quality = "poor";
  } else if (dropoutRate > 0.01) {
    quality = "degraded";
  } else {
    quality = "good";
  }

  return {
    dropouts: dropoutCount,
    frames: totalFrames,
    bufferMs: Math.round(bufferDepthMs),
    quality,
  };
}

// Expose for E2E testing (Playwright can access via page.evaluate)
if (typeof window !== "undefined") {
  window.__iem_audio_level = getAudioLevel;
  window.__iem_audio_error = getAudioError;
  window.__iem_audio_gain = getListenGain;
  window.__iem_stream_stats = getStreamStats;
}
