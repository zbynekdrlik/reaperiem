#pragma once
#include <JuceHeader.h>
#include "VBANProtocol.h"
#include "VBANPacket.h"
#include "../audio/AudioRingBuffer.h"
#include <atomic>

class VBANSender : public juce::Thread {
public:
    VBANSender();
    ~VBANSender() override;

    void configure(const juce::String& destIP, uint16_t port,
                   const juce::String& streamName,
                   int sampleRate, int numChannels);

    void start();
    void stop();

    // Called from audio thread to push samples for sending
    void pushSamples(const float* const* channelData, int numSamples);

    // Statistics
    uint64_t getPacketsSent() const { return packetsSent.load(); }
    uint64_t getBytesSent() const { return bytesSent.load(); }
    bool isRunning() const { return running.load(); }
    bool isSending() const { return sending.load(); }
    uint64_t getLastSendTime() const { return lastSendTime.load(); }

private:
    void run() override;

    juce::String destinationIP = "127.0.0.1";
    uint16_t port = vban::DEFAULT_PORT;
    juce::String streamName = "Stream1";

    vban::Packet packet;
    std::vector<uint8_t> packetBuffer;

    std::unique_ptr<AudioRingBuffer> ringBuffer;

    std::atomic<bool> running{false};
    std::atomic<bool> sending{false};
    std::atomic<uint64_t> packetsSent{0};
    std::atomic<uint64_t> bytesSent{0};
    std::atomic<uint64_t> lastSendTime{0};

    int sampleRate = 48000;
    int numChannels = 2;
    int samplesPerPacket = 256;

    juce::CriticalSection configLock;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(VBANSender)
};
