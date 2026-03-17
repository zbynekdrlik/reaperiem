//! Audio streaming pipeline: VBAN UDP → Resample → Opus → WebSocket
//!
//! Captures audio from REAPER via VBAN IEM VST3 (UDP packets on port 6980),
//! resamples from 96kHz to 48kHz, encodes to Opus, and broadcasts as binary frames.

use bytes::Bytes;
use rubato::Resampler;
use tokio::sync::broadcast;

/// VBAN magic bytes: "VBAN" as little-endian u32 = 0x4E414256
const VBAN_MAGIC: [u8; 4] = [0x56, 0x42, 0x41, 0x4E]; // "VBAN" in LE

/// VBAN header size: 28 bytes
/// 4 (magic) + 1 (format_SR) + 1 (format_nbs) + 1 (format_nbc) + 1 (format_bit)
/// + 16 (streamname) + 4 (nuFrame)
const VBAN_HEADER_SIZE: usize = 28;

/// Opus frame size in samples per channel at 48kHz (20ms)
const OPUS_FRAME_SAMPLES: usize = 960;

/// UDP bind address for receiving VBAN audio from the local VBAN IEM VST3 plugin.
/// Port 6980 is the standard VBAN port. Unlike ReaStream's hardcoded port 58710,
/// VBAN's port is configurable and doesn't conflict with REAPER.
/// No startup order dependency — plain bind, no SO_REUSEADDR needed.
const VBAN_BIND_ADDR: &str = "127.0.0.1:6980";

/// VBAN sample rate lookup table (21 entries, index 0-20)
const VBAN_SAMPLE_RATES: [u32; 21] = [
    6000, 12000, 24000, 48000, 96000, 192000, 384000, // 0-6
    8000, 16000, 32000, 64000, 128000, 256000, 512000, // 7-13
    11025, 22050, 44100, 88200, 176400, 352800, 705600, // 14-20
];

/// VBAN data type IDs (lower 3 bits of format_bit)
const VBAN_DATATYPE_INT16: u8 = 1;
const VBAN_DATATYPE_FLOAT32: u8 = 4;

/// Masks for parsing VBAN header fields
const VBAN_SR_MASK: u8 = 0x1F;
const VBAN_DATATYPE_MASK: u8 = 0x07;

/// Parsed VBAN packet
#[derive(Debug)]
pub struct VbanPacket {
    pub channels: u8,
    pub sample_rate: u32,
    pub samples_per_channel: u16,
    /// Interleaved float32 samples [L, R, L, R, ...]
    pub samples: Vec<f32>,
    pub stream_name: String,
}

/// Parse a VBAN UDP packet
///
/// VBAN format (little-endian):
/// - 4 bytes: magic 0x4E414256 ("VBAN" LE)
/// - 1 byte: format_SR — sample rate index (bits 0-4) | protocol (bits 5-7)
/// - 1 byte: format_nbs — samples per frame - 1 (0 = 1 sample, 255 = 256 samples)
/// - 1 byte: format_nbc — channels - 1 (0 = 1 channel, 255 = 256 channels)
/// - 1 byte: format_bit — data type (bits 0-2) | codec (bits 4-7)
/// - 16 bytes: stream name (null-padded ASCII)
/// - 4 bytes: frame counter (u32 LE)
/// - remaining: interleaved audio data (INT16 or FLOAT32)
pub fn parse_vban_packet(data: &[u8]) -> Result<VbanPacket, &'static str> {
    if data.len() < VBAN_HEADER_SIZE {
        return Err("packet too short");
    }

    // Check magic
    if data[0..4] != VBAN_MAGIC {
        return Err("wrong magic bytes");
    }

    // Parse header fields
    let format_sr = data[4];
    let format_nbs = data[5];
    let format_nbc = data[6];
    let format_bit = data[7];

    // Sample rate from index
    let sr_index = (format_sr & VBAN_SR_MASK) as usize;
    if sr_index >= VBAN_SAMPLE_RATES.len() {
        return Err("invalid sample rate index");
    }
    let sample_rate = VBAN_SAMPLE_RATES[sr_index];

    // Samples per frame = format_nbs + 1
    let samples_per_channel = (format_nbs as u16) + 1;

    // Channels = format_nbc + 1
    let channels = format_nbc + 1;
    if channels == 0 {
        return Err("invalid channel count");
    }

    // Data type
    let data_type = format_bit & VBAN_DATATYPE_MASK;

    // Stream name (16 bytes, null-terminated)
    let name_bytes = &data[8..24];
    let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(16);
    let stream_name = String::from_utf8_lossy(&name_bytes[..name_end]).to_string();

    // Calculate expected audio data size
    let total_samples = samples_per_channel as usize * channels as usize;
    let audio_data = &data[VBAN_HEADER_SIZE..];

    let samples = match data_type {
        VBAN_DATATYPE_INT16 => {
            let expected_bytes = total_samples * 2;
            if audio_data.len() < expected_bytes {
                return Err("truncated audio data");
            }
            let mut samples = Vec::with_capacity(total_samples);
            for i in 0..total_samples {
                let offset = i * 2;
                let raw = i16::from_le_bytes([audio_data[offset], audio_data[offset + 1]]);
                samples.push(raw as f32 / 32767.0);
            }
            samples
        }
        VBAN_DATATYPE_FLOAT32 => {
            let expected_bytes = total_samples * 4;
            if audio_data.len() < expected_bytes {
                return Err("truncated audio data");
            }
            let mut samples = Vec::with_capacity(total_samples);
            for i in 0..total_samples {
                let offset = i * 4;
                let sample = f32::from_le_bytes([
                    audio_data[offset],
                    audio_data[offset + 1],
                    audio_data[offset + 2],
                    audio_data[offset + 3],
                ]);
                samples.push(sample);
            }
            samples
        }
        _ => {
            return Err("unsupported data type");
        }
    };

    Ok(VbanPacket {
        channels,
        sample_rate,
        samples_per_channel,
        samples,
        stream_name,
    })
}

/// Audio processing pipeline: resample + buffer + encode
pub struct AudioPipeline {
    resampler: rubato::FftFixedIn<f32>,
    encoder: opus::Encoder,
    /// Accumulation buffer per channel for resampler input [channel][samples]
    channel_buffers: Vec<Vec<f32>>,
    /// Accumulation buffer for resampled interleaved output awaiting Opus encoding
    opus_buffer: Vec<f32>,
    channels: usize,
}

impl AudioPipeline {
    /// Create a new pipeline for the given input sample rate and channel count
    pub fn new(input_sample_rate: u32, channels: u8) -> Result<Self, String> {
        let ch = channels as usize;

        // Create resampler: input_rate -> 48kHz
        let resampler = rubato::FftFixedIn::<f32>::new(
            input_sample_rate as usize,
            48000,
            1024, // chunk size
            1,    // sub chunks
            ch,
        )
        .map_err(|e| format!("resampler init failed: {}", e))?;

        // Create Opus encoder: 48kHz, stereo, audio application
        let opus_channels = if ch >= 2 {
            opus::Channels::Stereo
        } else {
            opus::Channels::Mono
        };
        let mut encoder = opus::Encoder::new(48000, opus_channels, opus::Application::Audio)
            .map_err(|e| format!("opus encoder init failed: {}", e))?;

        // Set bitrate to 64kbps
        encoder
            .set_bitrate(opus::Bitrate::Bits(64000))
            .map_err(|e| format!("opus set bitrate failed: {}", e))?;

        let channel_buffers = vec![Vec::new(); ch];

        Ok(Self {
            resampler,
            encoder,
            channel_buffers,
            opus_buffer: Vec::new(),
            channels: ch,
        })
    }

    /// Process interleaved PCM samples, returning zero or more encoded Opus frames
    pub fn process(&mut self, interleaved: &[f32]) -> Vec<Bytes> {
        // De-interleave into per-channel buffers
        let samples_per_channel = interleaved.len() / self.channels;
        for i in 0..samples_per_channel {
            for ch in 0..self.channels {
                self.channel_buffers[ch].push(interleaved[i * self.channels + ch]);
            }
        }

        let mut opus_frames = Vec::new();

        // Feed resampler in chunks
        let chunk_size = self.resampler.input_frames_next();
        while self.channel_buffers[0].len() >= chunk_size {
            // Extract chunk from each channel
            let input: Vec<Vec<f32>> = self
                .channel_buffers
                .iter_mut()
                .map(|buf| buf.drain(..chunk_size).collect())
                .collect();

            // Resample
            let resampled: Vec<Vec<f32>> = match self.resampler.process(&input, None) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("resample error: {}", e);
                    continue;
                }
            };

            // Re-interleave the resampled output into the persistent opus buffer
            let out_frames = resampled[0].len();
            for i in 0..out_frames {
                for ch_buf in &resampled {
                    self.opus_buffer
                        .push(if i < ch_buf.len() { ch_buf[i] } else { 0.0 });
                }
            }
        }

        // Encode Opus frames from accumulated resampled data
        let frame_size = OPUS_FRAME_SAMPLES * self.channels;
        while self.opus_buffer.len() >= frame_size {
            let frame: Vec<f32> = self.opus_buffer.drain(..frame_size).collect();
            let mut output = vec![0u8; 4000]; // max Opus packet
            match self.encoder.encode_float(&frame, &mut output) {
                Ok(len) => {
                    output.truncate(len);
                    opus_frames.push(Bytes::from(output));
                }
                Err(e) => {
                    tracing::warn!("opus encode error: {}", e);
                }
            }
        }

        opus_frames
    }
}

/// Spawn the UDP listener that captures VBAN packets and broadcasts Opus frames
pub fn spawn_audio_listener(audio_tx: broadcast::Sender<Bytes>) {
    tokio::spawn(async move {
        // Plain bind — no SO_REUSEADDR needed. VBAN IEM VST sends to 127.0.0.1:6980
        // and there's no port conflict with REAPER (unlike ReaStream's hardcoded 58710).
        let socket = match tokio::net::UdpSocket::bind(VBAN_BIND_ADDR).await {
            Ok(s) => {
                tracing::info!("Audio listener bound to {}", VBAN_BIND_ADDR);
                s
            }
            Err(e) => {
                tracing::error!(
                    "Failed to bind audio UDP socket on {}: {}",
                    VBAN_BIND_ADDR,
                    e
                );
                return;
            }
        };

        let mut buf = vec![0u8; 65536];
        let mut pipeline: Option<AudioPipeline> = None;

        loop {
            let len = match socket.recv(&mut buf).await {
                Ok(len) => len,
                Err(e) => {
                    tracing::warn!("UDP recv error: {}", e);
                    continue;
                }
            };

            let packet = match parse_vban_packet(&buf[..len]) {
                Ok(p) => p,
                Err(e) => {
                    tracing::debug!("VBAN parse error: {}", e);
                    continue;
                }
            };

            // Lazily initialize pipeline on first valid packet
            let pipe = match pipeline.as_mut() {
                Some(p) => p,
                None => match AudioPipeline::new(packet.sample_rate, packet.channels) {
                    Ok(p) => {
                        tracing::info!(
                            sample_rate = packet.sample_rate,
                            channels = packet.channels,
                            stream_name = %packet.stream_name,
                            "Audio pipeline initialized (VBAN)"
                        );
                        pipeline = Some(p);
                        pipeline.as_mut().unwrap()
                    }
                    Err(e) => {
                        tracing::error!("Failed to init audio pipeline: {}", e);
                        continue;
                    }
                },
            };

            // Process and broadcast Opus frames
            let frames = pipe.process(&packet.samples);
            for frame in frames {
                // Ignore send errors (no subscribers)
                let _ = audio_tx.send(frame);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a VBAN packet with INT16 audio data for testing
    fn build_vban_packet(
        channels: u8,
        sample_rate: u32,
        samples_per_channel: u16,
        samples: &[f32],
        stream_name: &str,
    ) -> Vec<u8> {
        let mut packet = Vec::new();

        // Magic "VBAN" (LE)
        packet.extend_from_slice(&VBAN_MAGIC);

        // format_SR: sample rate index | protocol (audio = 0x00)
        let sr_index = VBAN_SAMPLE_RATES
            .iter()
            .position(|&r| r == sample_rate)
            .unwrap_or(3) as u8;
        packet.push(sr_index); // protocol audio = 0x00 in upper bits

        // format_nbs: samples per frame - 1
        packet.push((samples_per_channel - 1) as u8);

        // format_nbc: channels - 1
        packet.push(channels - 1);

        // format_bit: INT16 (1) | codec PCM (0x00)
        packet.push(VBAN_DATATYPE_INT16);

        // Stream name (16 bytes, null-padded)
        let name_bytes = stream_name.as_bytes();
        let name_len = name_bytes.len().min(16);
        packet.extend_from_slice(&name_bytes[..name_len]);
        packet.extend(std::iter::repeat(0u8).take(16 - name_len));

        // Frame counter (u32 LE)
        packet.extend_from_slice(&0u32.to_le_bytes());

        // Audio data: interleaved INT16
        for &s in samples {
            let clamped = s.clamp(-1.0, 1.0);
            let int16_val = (clamped * 32767.0) as i16;
            packet.extend_from_slice(&int16_val.to_le_bytes());
        }

        packet
    }

    /// Build a VBAN packet with FLOAT32 audio data for testing
    fn build_vban_packet_f32(
        channels: u8,
        sample_rate: u32,
        samples_per_channel: u16,
        samples: &[f32],
        stream_name: &str,
    ) -> Vec<u8> {
        let mut packet = Vec::new();

        // Magic "VBAN" (LE)
        packet.extend_from_slice(&VBAN_MAGIC);

        // format_SR
        let sr_index = VBAN_SAMPLE_RATES
            .iter()
            .position(|&r| r == sample_rate)
            .unwrap_or(3) as u8;
        packet.push(sr_index);

        // format_nbs
        packet.push((samples_per_channel - 1) as u8);

        // format_nbc
        packet.push(channels - 1);

        // format_bit: FLOAT32 (4) | codec PCM (0x00)
        packet.push(VBAN_DATATYPE_FLOAT32);

        // Stream name
        let name_bytes = stream_name.as_bytes();
        let name_len = name_bytes.len().min(16);
        packet.extend_from_slice(&name_bytes[..name_len]);
        packet.extend(std::iter::repeat(0u8).take(16 - name_len));

        // Frame counter
        packet.extend_from_slice(&0u32.to_le_bytes());

        // Audio data: interleaved FLOAT32
        for &s in samples {
            packet.extend_from_slice(&s.to_le_bytes());
        }

        packet
    }

    #[test]
    fn test_parse_valid_int16_packet() {
        // 2 channels, 96kHz, 4 samples per channel = 8 interleaved samples
        let samples = vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3, 0.4, -0.4];
        let packet = build_vban_packet(2, 96000, 4, &samples, "engineer");

        let parsed = parse_vban_packet(&packet).unwrap();
        assert_eq!(parsed.channels, 2);
        assert_eq!(parsed.sample_rate, 96000);
        assert_eq!(parsed.samples_per_channel, 4);
        assert_eq!(parsed.samples.len(), 8);
        assert_eq!(parsed.stream_name, "engineer");
        // INT16 conversion has quantization error, check within tolerance
        assert!((parsed.samples[0] - 0.1).abs() < 0.001);
        assert!((parsed.samples[1] - (-0.1)).abs() < 0.001);
    }

    #[test]
    fn test_parse_valid_float32_packet() {
        let samples = vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3, 0.4, -0.4];
        let packet = build_vban_packet_f32(2, 96000, 4, &samples, "engineer");

        let parsed = parse_vban_packet(&packet).unwrap();
        assert_eq!(parsed.channels, 2);
        assert_eq!(parsed.sample_rate, 96000);
        assert_eq!(parsed.samples.len(), 8);
        // FLOAT32 is lossless
        assert!((parsed.samples[0] - 0.1).abs() < 1e-6);
        assert!((parsed.samples[1] - (-0.1)).abs() < 1e-6);
    }

    #[test]
    fn test_parse_wrong_magic() {
        let mut packet = build_vban_packet(2, 96000, 4, &[0.0; 8], "test");
        packet[0] = b'X'; // corrupt magic
        assert_eq!(parse_vban_packet(&packet).unwrap_err(), "wrong magic bytes");
    }

    #[test]
    fn test_parse_truncated_header() {
        let packet = &VBAN_MAGIC; // only magic, no header fields
        assert_eq!(parse_vban_packet(packet).unwrap_err(), "packet too short");
    }

    #[test]
    fn test_parse_truncated_audio() {
        // Build a packet claiming 4 samples/ch but provide only 2 samples worth of data
        let mut packet = build_vban_packet(2, 96000, 2, &[0.0; 4], "test");
        // Overwrite format_nbs to claim 4 samples per channel (index 3 = 4 samples)
        packet[5] = 3; // nbs = 3 means 4 samples per channel
        assert_eq!(parse_vban_packet(&packet).unwrap_err(), "truncated audio data");
    }

    #[test]
    fn test_parse_mono_packet() {
        let samples = vec![0.5, 0.6, 0.7, 0.8];
        let packet = build_vban_packet(1, 48000, 4, &samples, "mono");
        let parsed = parse_vban_packet(&packet).unwrap();
        assert_eq!(parsed.channels, 1);
        assert_eq!(parsed.sample_rate, 48000);
        assert_eq!(parsed.samples.len(), 4);
    }

    #[test]
    fn test_int16_to_f32_conversion() {
        // Verify INT16 → f32 conversion accuracy
        let samples = vec![1.0, -1.0, 0.0, 0.5];
        let packet = build_vban_packet(1, 48000, 4, &samples, "test");
        let parsed = parse_vban_packet(&packet).unwrap();

        // 1.0 → 32767 → 32767/32767.0 = 1.0 (exact)
        assert!((parsed.samples[0] - 1.0).abs() < 0.001);
        // -1.0 → -32767 → -32767/32767.0 ≈ -1.0
        assert!((parsed.samples[1] - (-1.0)).abs() < 0.001);
        // 0.0 → 0 → 0.0 (exact)
        assert!((parsed.samples[2]).abs() < 0.001);
        // 0.5 → 16383 → 16383/32767.0 ≈ 0.5
        assert!((parsed.samples[3] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_sample_rate_lookup() {
        // Test all sample rates in the lookup table
        for (idx, &rate) in VBAN_SAMPLE_RATES.iter().enumerate() {
            let samples = vec![0.0; 2];
            let packet = build_vban_packet(1, rate, 2, &samples, "test");
            let parsed = parse_vban_packet(&packet).unwrap();
            assert_eq!(
                parsed.sample_rate, rate,
                "Sample rate mismatch at index {}",
                idx
            );
        }
    }

    #[test]
    fn test_stream_name_parsing() {
        let packet = build_vban_packet(1, 48000, 1, &[0.0], "engineer");
        let parsed = parse_vban_packet(&packet).unwrap();
        assert_eq!(parsed.stream_name, "engineer");

        // Full 16-char name
        let packet = build_vban_packet(1, 48000, 1, &[0.0], "1234567890123456");
        let parsed = parse_vban_packet(&packet).unwrap();
        // Name is truncated to 15 chars + null in build_vban_packet (16 bytes total)
        assert_eq!(parsed.stream_name.len(), 16);
    }

    #[test]
    fn test_unsupported_data_type() {
        let mut packet = build_vban_packet(1, 48000, 1, &[0.0], "test");
        // Set data type to INT24 (2) which we don't support
        packet[7] = 2;
        assert_eq!(parse_vban_packet(&packet).unwrap_err(), "unsupported data type");
    }

    #[test]
    fn test_pipeline_resamples_96k_to_48k() {
        // Create pipeline for 96kHz stereo
        let mut pipeline = AudioPipeline::new(96000, 2).unwrap();

        // Feed enough data: resampler chunk=1024, ratio=2:1, Opus frame=960
        let num_samples = 96000;
        let mut total_frames = 0;

        for chunk_start in (0..num_samples).step_by(1024) {
            let chunk_end = (chunk_start + 1024).min(num_samples);
            let mut interleaved = Vec::new();
            for i in chunk_start..chunk_end {
                let t = i as f32 / 96000.0;
                let val = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
                interleaved.push(val); // L
                interleaved.push(val); // R
            }
            let frames = pipeline.process(&interleaved);
            total_frames += frames.len();

            for frame in &frames {
                assert!(!frame.is_empty(), "Opus frame should not be empty");
                assert!(frame.len() < 4000, "Opus frame should be reasonable size");
            }
        }

        // 96000 samples @ 96kHz = 1 second → 48000 @ 48kHz → ~50 Opus frames (20ms each)
        assert!(
            total_frames >= 1,
            "Should produce Opus frames from 1 second of 96kHz audio, got {}",
            total_frames
        );
    }

    #[test]
    fn test_opus_encode_decode_roundtrip() {
        // Create encoder
        let mut encoder =
            opus::Encoder::new(48000, opus::Channels::Stereo, opus::Application::Audio).unwrap();
        encoder.set_bitrate(opus::Bitrate::Bits(64000)).unwrap();

        // Create decoder
        let mut decoder = opus::Decoder::new(48000, opus::Channels::Stereo).unwrap();

        // Encode and decode multiple frames to let the codec converge
        let num_frames = 20;
        let mut last_input = vec![0.0f32; 960 * 2];
        let mut last_decoded = vec![0.0f32; 960 * 2];

        for frame_idx in 0..num_frames {
            let mut input = vec![0.0f32; 960 * 2];
            for i in 0..960 {
                let sample_offset = frame_idx * 960 + i;
                let t = sample_offset as f32 / 48000.0;
                let val = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
                input[i * 2] = val;
                input[i * 2 + 1] = val;
            }

            let mut encoded = vec![0u8; 4000];
            let len = encoder.encode_float(&input, &mut encoded).unwrap();
            encoded.truncate(len);

            assert!(len > 0, "Encoded data should not be empty");

            let mut decoded = vec![0.0f32; 960 * 2];
            let decoded_samples = decoder.decode_float(&encoded, &mut decoded, false).unwrap();
            assert_eq!(
                decoded_samples, 960,
                "Should decode 960 samples per channel"
            );

            last_input = input;
            last_decoded = decoded;
        }

        // Check correlation on the last frame (after codec warmup)
        let mut dot = 0.0f64;
        let mut norm_in = 0.0f64;
        let mut norm_out = 0.0f64;
        for i in 0..960 {
            let a = last_input[i * 2] as f64;
            let b = last_decoded[i * 2] as f64;
            dot += a * b;
            norm_in += a * a;
            norm_out += b * b;
        }
        let correlation = dot / (norm_in.sqrt() * norm_out.sqrt());
        assert!(
            correlation > 0.5,
            "Correlation between input and decoded output should be > 0.5, got {}",
            correlation
        );
    }

    #[tokio::test]
    async fn test_udp_listener_receives_vban_packet() {
        // Test the full server-side pipeline: UDP → parse → resample → encode → broadcast
        let (audio_tx, mut audio_rx) = broadcast::channel::<Bytes>(64);

        // Bind listener on an ephemeral port to avoid conflicts
        let listener_socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let listener_addr = listener_socket.local_addr().unwrap();

        // Spawn a custom listener task (same logic as spawn_audio_listener but with our socket)
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            let mut pipeline: Option<AudioPipeline> = None;

            loop {
                let len = match listener_socket.recv(&mut buf).await {
                    Ok(len) => len,
                    Err(_) => continue,
                };

                let packet = match parse_vban_packet(&buf[..len]) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let pipe = match pipeline.as_mut() {
                    Some(p) => p,
                    None => match AudioPipeline::new(packet.sample_rate, packet.channels) {
                        Ok(p) => {
                            pipeline = Some(p);
                            pipeline.as_mut().unwrap()
                        }
                        Err(_) => continue,
                    },
                };

                let frames = pipe.process(&packet.samples);
                for frame in frames {
                    let _ = audio_tx.send(frame);
                }
            }
        });

        // Give listener time to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Create a sender socket and send enough data to produce Opus frames
        let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender.connect(listener_addr).await.unwrap();

        // Send ~1 second of 96kHz stereo audio as VBAN INT16 packets
        // VBAN max samples per packet: 256 (format_nbs is u8, max 255 = 256 samples)
        let samples_per_packet = 256;
        let total_packets = (96000 / samples_per_packet) + 1;

        for pkt_idx in 0..total_packets {
            let mut samples = Vec::with_capacity(samples_per_packet * 2);
            for i in 0..samples_per_packet {
                let sample_offset = pkt_idx * samples_per_packet + i;
                let t = sample_offset as f32 / 96000.0;
                let val = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.3;
                samples.push(val); // L
                samples.push(val); // R
            }
            let packet =
                build_vban_packet(2, 96000, samples_per_packet as u16, &samples, "engineer");
            sender.send(&packet).await.unwrap();

            // Small delay to avoid overwhelming the receiver
            if pkt_idx % 20 == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }

        // Wait for pipeline to process all packets
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Collect all Opus frames from the broadcast channel
        let mut frame_count = 0;
        while let Ok(frame) = audio_rx.try_recv() {
            assert!(!frame.is_empty(), "Opus frame should not be empty");
            frame_count += 1;
        }

        // 1 second of 96kHz → ~48000 samples at 48kHz → ~50 Opus frames (20ms each)
        assert!(
            frame_count >= 10,
            "Expected at least 10 Opus frames from ~1s of audio, got {}",
            frame_count
        );
    }

    #[test]
    fn test_bind_address_uses_vban_port() {
        let addr: std::net::SocketAddr = VBAN_BIND_ADDR.parse().unwrap();
        assert_eq!(addr.port(), 6980, "Must bind to VBAN port 6980");
        assert!(
            addr.ip().is_loopback(),
            "VBAN_BIND_ADDR must be 127.0.0.1 (localhost only), got {}",
            addr.ip()
        );
    }

    #[test]
    fn test_pipeline_processes_incrementally() {
        // Pipeline should buffer small packets and output frames when enough data accumulates
        let mut pipeline = AudioPipeline::new(48000, 2).unwrap();

        // Feed small chunks (128 samples per channel)
        let mut total_frames = 0;
        for _ in 0..20 {
            let chunk: Vec<f32> = (0..256).map(|i| (i as f32 * 0.001).sin()).collect();
            let frames = pipeline.process(&chunk);
            total_frames += frames.len();
        }

        // 20 * 128 = 2560 samples per channel at 48kHz
        // = 2560 / 960 ≈ 2 full Opus frames
        assert!(
            total_frames >= 1,
            "Should produce at least 1 Opus frame from 2560 input samples, got {}",
            total_frames
        );
    }
}
