#pragma once
#include <JuceHeader.h>
#include "VBANProtocol.h"
#include <vector>

namespace vban {

class Packet {
public:
    Packet();

    // Configure packet header
    void configure(int sampleRate, int numChannels, DataType dataType, const juce::String& streamName);

    // Serialize audio samples into VBAN packet bytes
    // Returns the number of bytes written to packetData
    int serialize(const float* const* channelData, int numSamples, int numChannels,
                  uint8_t* packetData, size_t maxPacketSize);

    // Deserialize VBAN packet bytes into audio samples
    // Returns the number of samples decoded
    int deserialize(const uint8_t* packetData, size_t packetSize,
                    float* interleavedOutput, int maxSamples, int& outNumChannels);

    // Get frame counter and increment
    uint32_t getAndIncrementFrameCounter();

    // Validate and parse header from packet data
    static bool parseHeader(const uint8_t* packetData, size_t packetSize, Header& outHeader);

    // Get max samples per packet based on channels and data type
    static int getMaxSamplesPerPacket(int numChannels, DataType dataType);

private:
    Header header;
    DataType dataType = DataType::INT16;
    uint32_t frameCounter = 0;

    // Conversion helpers
    static int16_t floatToInt16(float sample);
    static float int16ToFloat(int16_t sample);

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(Packet)
};

} // namespace vban
