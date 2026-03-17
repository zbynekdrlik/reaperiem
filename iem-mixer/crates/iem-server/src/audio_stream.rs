//! Audio streaming pipeline: ReaStream UDP → Resample → Opus → WebSocket
//!
//! Captures audio from REAPER via ReaStream VST (UDP packets on port 58710),
//! resamples from 96kHz to 48kHz, encodes to Opus, and broadcasts as binary frames.

use bytes::Bytes;
use rubato::Resampler;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::sync::broadcast;

/// ReaStream magic bytes: "MRSR" (little-endian u32)
const REASTREAM_MAGIC: &[u8; 4] = b"MRSR";

/// ReaStream header size: 4 (magic) + 4 (packet_size) + 32 (identifier) + 1 (channels) + 4 (sample_rate) + 2 (block_size)
const REASTREAM_HEADER_SIZE: usize = 47;

/// Opus frame size in samples per channel at 48kHz (20ms)
const OPUS_FRAME_SAMPLES: usize = 960;

/// UDP bind address for receiving ReaStream audio.
/// We bind to 0.0.0.0:58710 (ReaStream's default port) WITH SO_REUSEADDR.
/// The app MUST start BEFORE REAPER so we bind first. When ReaStream initializes
/// later, it also binds 0.0.0.0:58710 with SO_REUSEADDR. On Windows, the
/// first-bound socket gets priority for incoming packets. ReaStream sends to
/// 127.0.0.1:58710, and our socket (bound first) receives them.
/// CI deploy ensures correct order: stop REAPER → start app → restart REAPER.
/// At boot, the startup launcher runs before the user opens REAPER.
const REASTREAM_BIND_ADDR: &str = "0.0.0.0:58710";

/// Parsed ReaStream packet
#[derive(Debug)]
pub struct ReaStreamPacket {
    pub channels: u8,
    pub sample_rate: u32,
    pub block_size: u16,
    /// Interleaved float32 samples [L, R, L, R, ...]
    pub samples: Vec<f32>,
}

/// Parse a ReaStream UDP packet
///
/// ReaStream format (little-endian):
/// - 4 bytes: magic "MRSR"
/// - 4 bytes: packet_size (total size after magic)
/// - 32 bytes: identifier (null-padded ASCII string)
/// - 1 byte: channel count
/// - 4 bytes: sample rate (u32)
/// - 2 bytes: block_size (samples per channel in this packet)
/// - remaining: interleaved float32 PCM samples
pub fn parse_reastream_packet(data: &[u8]) -> Result<ReaStreamPacket, &'static str> {
    if data.len() < REASTREAM_HEADER_SIZE {
        return Err("packet too short");
    }

    // Check magic
    if &data[0..4] != REASTREAM_MAGIC {
        return Err("wrong magic bytes");
    }

    // Parse header fields
    let channels = data[40];
    if channels == 0 || channels > 8 {
        return Err("invalid channel count");
    }

    let sample_rate = u32::from_le_bytes([data[41], data[42], data[43], data[44]]);
    if sample_rate == 0 {
        return Err("invalid sample rate");
    }

    let block_size = u16::from_le_bytes([data[45], data[46]]);
    if block_size == 0 {
        return Err("invalid block size");
    }

    let expected_samples = block_size as usize * channels as usize;
    let expected_bytes = expected_samples * 4; // f32 = 4 bytes
    let audio_data = &data[REASTREAM_HEADER_SIZE..];

    if audio_data.len() < expected_bytes {
        return Err("truncated audio data");
    }

    let mut samples = Vec::with_capacity(expected_samples);
    for i in 0..expected_samples {
        let offset = i * 4;
        let sample = f32::from_le_bytes([
            audio_data[offset],
            audio_data[offset + 1],
            audio_data[offset + 2],
            audio_data[offset + 3],
        ]);
        samples.push(sample);
    }

    Ok(ReaStreamPacket {
        channels,
        sample_rate,
        block_size,
        samples,
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

/// Spawn the UDP listener that captures ReaStream packets and broadcasts Opus frames
pub fn spawn_audio_listener(audio_tx: broadcast::Sender<Bytes>) {
    tokio::spawn(async move {
        // Use socket2 to set SO_REUSEADDR before binding. This allows ReaStream
        // (which starts later) to also bind 0.0.0.0:58710. The first-bound socket
        // (ours) gets priority for incoming packets on Windows.
        let socket = match (|| -> Result<tokio::net::UdpSocket, Box<dyn std::error::Error>> {
            let addr: std::net::SocketAddr = REASTREAM_BIND_ADDR.parse()?;
            let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            sock.set_reuse_address(true)?;
            sock.bind(&addr.into())?;
            sock.set_nonblocking(true)?;
            let std_socket: std::net::UdpSocket = sock.into();
            Ok(tokio::net::UdpSocket::from_std(std_socket)?)
        })() {
            Ok(s) => {
                tracing::info!(
                    "Audio listener bound to {} (SO_REUSEADDR, must start before REAPER)",
                    REASTREAM_BIND_ADDR
                );
                s
            }
            Err(e) => {
                tracing::error!("Failed to bind audio UDP socket: {}", e);
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

            let packet = match parse_reastream_packet(&buf[..len]) {
                Ok(p) => p,
                Err(e) => {
                    tracing::debug!("ReaStream parse error: {}", e);
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
                            "Audio pipeline initialized"
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

    /// Build a minimal valid ReaStream packet for testing
    fn build_reastream_packet(
        channels: u8,
        sample_rate: u32,
        samples_per_channel: u16,
        samples: &[f32],
    ) -> Vec<u8> {
        let mut packet = Vec::new();
        // Magic
        packet.extend_from_slice(REASTREAM_MAGIC);
        // Packet size (everything after magic)
        let audio_bytes = samples.len() * 4;
        let packet_size = 32 + 1 + 4 + 2 + audio_bytes;
        packet.extend_from_slice(&(packet_size as u32).to_le_bytes());
        // Identifier (32 bytes, null-padded)
        let id = b"engineer";
        packet.extend_from_slice(id);
        packet.extend(std::iter::repeat(0u8).take(32 - id.len()));
        // Channels
        packet.push(channels);
        // Sample rate
        packet.extend_from_slice(&sample_rate.to_le_bytes());
        // Block size (samples per channel)
        packet.extend_from_slice(&samples_per_channel.to_le_bytes());
        // Audio data (interleaved f32)
        for &s in samples {
            packet.extend_from_slice(&s.to_le_bytes());
        }
        packet
    }

    #[test]
    fn test_parse_valid_packet() {
        // 2 channels, 96kHz, 4 samples per channel = 8 interleaved samples
        let samples = vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3, 0.4, -0.4];
        let packet = build_reastream_packet(2, 96000, 4, &samples);

        let parsed = parse_reastream_packet(&packet).unwrap();
        assert_eq!(parsed.channels, 2);
        assert_eq!(parsed.sample_rate, 96000);
        assert_eq!(parsed.block_size, 4);
        assert_eq!(parsed.samples.len(), 8);
        assert!((parsed.samples[0] - 0.1).abs() < 1e-6);
        assert!((parsed.samples[1] - (-0.1)).abs() < 1e-6);
    }

    #[test]
    fn test_parse_wrong_magic() {
        let mut packet = build_reastream_packet(2, 96000, 4, &[0.0; 8]);
        packet[0] = b'X'; // corrupt magic
        assert_eq!(
            parse_reastream_packet(&packet).unwrap_err(),
            "wrong magic bytes"
        );
    }

    #[test]
    fn test_parse_truncated_header() {
        let packet = b"MRSR"; // only magic, no header
        assert_eq!(
            parse_reastream_packet(packet).unwrap_err(),
            "packet too short"
        );
    }

    #[test]
    fn test_parse_truncated_audio() {
        // Claim 4 samples per channel (8 floats = 32 bytes) but provide only 2 floats
        let mut packet = build_reastream_packet(2, 96000, 4, &[0.0; 2]);
        // Fix block_size to 4 even though we only have 2 samples
        packet[45] = 4;
        packet[46] = 0;
        assert_eq!(
            parse_reastream_packet(&packet).unwrap_err(),
            "truncated audio data"
        );
    }

    #[test]
    fn test_parse_zero_channels() {
        let mut packet = build_reastream_packet(1, 96000, 1, &[0.0]);
        packet[40] = 0; // zero channels
        assert_eq!(
            parse_reastream_packet(&packet).unwrap_err(),
            "invalid channel count"
        );
    }

    #[test]
    fn test_parse_mono_packet() {
        let samples = vec![0.5, 0.6, 0.7, 0.8];
        let packet = build_reastream_packet(1, 48000, 4, &samples);
        let parsed = parse_reastream_packet(&packet).unwrap();
        assert_eq!(parsed.channels, 1);
        assert_eq!(parsed.sample_rate, 48000);
        assert_eq!(parsed.samples.len(), 4);
    }

    #[test]
    fn test_pipeline_resamples_96k_to_48k() {
        // Create pipeline for 96kHz stereo
        let mut pipeline = AudioPipeline::new(96000, 2).unwrap();

        // Feed enough data: resampler chunk=1024, ratio=2:1, Opus frame=960
        // Need multiple resampler chunks worth of data to produce output
        // Feed 96000 samples (1 second) in large chunks to ensure pipeline produces frames
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
        // (first frames have codec startup artifacts, correlation improves after warmup)
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
    async fn test_udp_listener_receives_reastream_packet() {
        // Test the full server-side pipeline: UDP → parse → resample → encode → broadcast
        //
        // 1. Create a broadcast channel
        // 2. Spawn the UDP listener on a test port
        // 3. Send synthetic ReaStream packets
        // 4. Verify Opus frames arrive on the broadcast receiver

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

                let packet = match parse_reastream_packet(&buf[..len]) {
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

        // Send ~1 second of 96kHz stereo audio as ReaStream packets
        // Each packet: 512 samples per channel (typical ReaStream block size)
        let samples_per_packet = 512;
        let total_packets = (96000 / samples_per_packet) + 1; // >1 second

        for pkt_idx in 0..total_packets {
            let mut samples = Vec::with_capacity(samples_per_packet * 2);
            for i in 0..samples_per_packet {
                let sample_offset = pkt_idx * samples_per_packet + i;
                let t = sample_offset as f32 / 96000.0;
                let val = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.3;
                samples.push(val); // L
                samples.push(val); // R
            }
            let packet = build_reastream_packet(2, 96000, samples_per_packet as u16, &samples);
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
        // Allow some margin due to resampler chunk boundaries
        assert!(
            frame_count >= 10,
            "Expected at least 10 Opus frames from ~1s of audio, got {}",
            frame_count
        );
    }

    #[test]
    fn test_bind_address_uses_reastream_port() {
        // We bind to ReaStream's default port 58710 on 0.0.0.0.
        // The app MUST start before REAPER to get exclusive ownership.
        // No SO_REUSEADDR — plain bind ensures we're the sole owner.
        let addr: std::net::SocketAddr = REASTREAM_BIND_ADDR.parse().unwrap();
        assert_eq!(
            addr.port(),
            58710,
            "Must bind to ReaStream default port 58710"
        );
        assert!(
            addr.ip().is_unspecified(),
            "REASTREAM_BIND_ADDR must be 0.0.0.0, got {}",
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
