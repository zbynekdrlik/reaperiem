#pragma once
#include <JuceHeader.h>
#include <vector>
#include <cstdint>
#include <atomic>

/// Receives OIEM UDP packets containing Opus-encoded audio.
/// Mirror of OpusSender, but for the receive direction.
///
/// UDP thread: binds ephemeral port, sends heartbeat to 127.0.0.1:7980 every 2s,
/// receives OIEM packets, pushes raw Opus frames to SPSC FIFO.
/// Audio thread: calls popFrame() to consume frames (lock-free).
///
/// OIEM packet format:
///   Bytes 0-3: Magic "OIEM" (0x4F49454D)
///   Bytes 4-5: Sequence number (uint16 LE, wrapping)
///   Bytes 6-7: Payload size in bytes (uint16 LE)
///   Bytes 8+:  Raw Opus frame
class OpusReceiver : public juce::Thread {
public:
    OpusReceiver();
    ~OpusReceiver() override;

    /// Configure the server port to send heartbeats to. Must be called before start().
    void configure(const juce::String& serverIP, uint16_t serverPort);

    void start();
    void stop();

    /// Pop a received Opus frame from the FIFO (lock-free, audio-thread safe).
    /// Returns the frame size in bytes, or 0 if no frame available.
    /// Data is written to `outData` which must have space for at least `maxSize` bytes.
    int popFrame(uint8_t* outData, int maxSize);

    // Statistics
    uint64_t getPacketsReceived() const { return packetsReceived.load(); }
    bool isReceiving() const { return receiving.load(); }
    uint16_t getLocalPort() const { return boundPort.load(); }

    /// OIEM magic bytes (same as OpusSender)
    static constexpr uint8_t OIEM_MAGIC[4] = { 'O', 'I', 'E', 'M' };
    static constexpr int OIEM_HEADER_SIZE = 8;
    static constexpr int HEARTBEAT_INTERVAL_MS = 2000;

private:
    void run() override;
    void sendHeartbeat(juce::DatagramSocket& sock);

    juce::String serverIP = "127.0.0.1";
    uint16_t serverPort = 7980;

    /// Lock-free SPSC FIFO for received Opus frames (byte-oriented)
    /// Each entry: [2 bytes frame_size LE] [frame_size bytes data]
    static constexpr int FIFO_CAPACITY = 32768; // ~50 Opus frames worth
    juce::AbstractFifo fifo{FIFO_CAPACITY};
    std::vector<uint8_t> fifoBuffer;

    std::atomic<bool> running{false};
    std::atomic<bool> receiving{false};
    std::atomic<uint64_t> packetsReceived{0};
    std::atomic<uint16_t> boundPort{0};

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(OpusReceiver)
};
