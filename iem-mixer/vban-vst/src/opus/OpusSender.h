#pragma once
#include <JuceHeader.h>
#include <vector>
#include <cstdint>
#include <atomic>

/// OIEM UDP packet format:
///   Bytes 0-3: Magic "OIEM" (0x4F49454D little-endian)
///   Bytes 4-5: Sequence number (uint16 LE, wrapping)
///   Bytes 6-7: Payload size in bytes (uint16 LE)
///   Bytes 8+:  Raw Opus frame
///
/// Sends encoded Opus frames over UDP using a lock-free SPSC queue.
/// Audio thread pushes frames, sender thread sends them via UDP.
class OpusSender : public juce::Thread {
public:
    OpusSender();
    ~OpusSender() override;

    /// Configure destination. Must be called before start().
    void configure(const juce::String& destIP, uint16_t port);

    void start();
    void stop();

    /// Push an encoded Opus frame from the audio thread (lock-free).
    /// If the queue is full, the frame is dropped (acceptable for real-time).
    void pushFrame(const uint8_t* data, int size);

    // Statistics
    uint64_t getPacketsSent() const { return packetsSent.load(); }
    bool isSending() const { return sending.load(); }

    /// OIEM magic bytes
    static constexpr uint8_t OIEM_MAGIC[4] = { 'O', 'I', 'E', 'M' };
    static constexpr int OIEM_HEADER_SIZE = 8;

private:
    void run() override;

    juce::String destinationIP = "127.0.0.1";
    uint16_t port = 6980;

    /// Lock-free SPSC FIFO for Opus frames (byte-oriented)
    /// Each entry: [2 bytes frame_size LE] [frame_size bytes data]
    static constexpr int FIFO_CAPACITY = 32768; // ~50 Opus frames worth
    juce::AbstractFifo fifo{FIFO_CAPACITY};
    std::vector<uint8_t> fifoBuffer;

    std::atomic<bool> running{false};
    std::atomic<bool> sending{false};
    std::atomic<uint64_t> packetsSent{0};
    std::atomic<uint16_t> sequenceNumber{0};

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(OpusSender)
};
