#include "VBANSender.h"

VBANSender::VBANSender()
    : Thread("VBAN Sender")
    , ringBuffer(std::make_unique<AudioRingBuffer>(64, 48000))  // Support up to 64 channels
{
    packetBuffer.resize(vban::MAX_PACKET_SIZE);
}

VBANSender::~VBANSender() {
    stop();
}

void VBANSender::configure(const juce::String& destIP, uint16_t port_,
                           const juce::String& streamName_,
                           int sampleRate_, int numChannels_) {
    juce::ScopedLock sl(configLock);

    destinationIP = destIP;
    port = port_;
    streamName = streamName_;
    sampleRate = sampleRate_;
    numChannels = std::clamp(numChannels_, 1, 64);  // Support up to 64 channels for hub mode

    // Configure packet header
    packet.configure(sampleRate, numChannels, vban::DataType::INT16, streamName);

    // Calculate optimal samples per packet
    samplesPerPacket = vban::Packet::getMaxSamplesPerPacket(numChannels, vban::DataType::INT16);
    samplesPerPacket = std::min(samplesPerPacket, 256); // Cap at 256 for lower latency

    // Resize ring buffer for ~50ms of audio (enough for callback jitter)
    ringBuffer->setSize(numChannels, sampleRate / 20);
}

void VBANSender::start() {
    if (!running.exchange(true)) {
        ringBuffer->reset();
        packetsSent = 0;
        bytesSent = 0;
        sending = false;
        lastSendTime = 0;
        startThread();
    }
}

void VBANSender::stop() {
    running = false;
    sending = false;
    stopThread(1000);
}

void VBANSender::pushSamples(const float* const* channelData, int numSamples) {
    if (!running.load()) return;

    juce::ScopedLock sl(configLock);
    ringBuffer->writeSamples(channelData, numChannels, numSamples);
}

void VBANSender::run() {
    // Create fresh socket for this session
    juce::DatagramSocket sock;

    // Bind socket to any available port
    if (!sock.bindToPort(0)) {
        DBG("VBANSender: Failed to bind socket");
        running = false;
        return;
    }

    // Temporary buffer for reading from ring buffer
    juce::AudioBuffer<float> tempBuffer(numChannels, samplesPerPacket);

    while (!threadShouldExit() && running.load()) {
        int available;
        {
            juce::ScopedLock sl(configLock);
            available = ringBuffer->getNumSamplesAvailableForReading();
        }

        if (available >= samplesPerPacket) {
            // Read samples from ring buffer
            {
                juce::ScopedLock sl(configLock);
                ringBuffer->readSamples(tempBuffer.getArrayOfWritePointers(), numChannels, samplesPerPacket);
            }

            // Serialize to VBAN packet
            int packetSize = packet.serialize(
                tempBuffer.getArrayOfReadPointers(),
                samplesPerPacket,
                numChannels,
                packetBuffer.data(),
                packetBuffer.size()
            );

            if (packetSize > 0) {
                // Send UDP packet
                int sent = sock.write(destinationIP, port, packetBuffer.data(), packetSize);
                if (sent > 0) {
                    packetsSent++;
                    bytesSent += sent;
                    sending = true;
                    lastSendTime = juce::Time::getMillisecondCounter();
                } else {
                    sending = false;
                }
            }
        } else {
            // Wait a bit for more samples
            Thread::sleep(1);
        }
    }

    // Socket automatically closed when sock goes out of scope
}
