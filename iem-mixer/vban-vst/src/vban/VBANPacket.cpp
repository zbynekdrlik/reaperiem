#include "VBANPacket.h"
#include <cstring>
#include <algorithm>

namespace vban {

Packet::Packet() {
    std::memset(&header, 0, sizeof(Header));
    header.magic = MAGIC;
}

void Packet::configure(int sampleRate, int numChannels, DataType dataType_, const juce::String& streamName) {
    dataType = dataType_;

    // Set sample rate index
    int srIndex = getSampleRateIndex(sampleRate);
    header.format_SR = static_cast<uint8_t>(srIndex & SR_MASK) | PROTOCOL_AUDIO;

    // Channels - 1 (0-255 = 1-256 channels)
    header.format_nbc = static_cast<uint8_t>(std::clamp(numChannels - 1, 0, 255));

    // Bit format and codec
    header.format_bit = static_cast<uint8_t>(dataType) | CODEC_PCM;

    // Stream name
    setStreamName(header, streamName.toRawUTF8());
}

int Packet::serialize(const float* const* channelData, int numSamples, int numChannels,
                      uint8_t* packetData, size_t maxPacketSize) {

    if (maxPacketSize < HEADER_SIZE) return 0;

    int channels = std::min(numChannels, static_cast<int>(header.format_nbc) + 1);
    int sampleSize = getSampleSize(dataType);
    int maxSamples = static_cast<int>((maxPacketSize - HEADER_SIZE) / (channels * sampleSize));
    int samplesToSend = std::min(numSamples, maxSamples);

    if (samplesToSend <= 0) return 0;

    // Update header
    header.format_nbs = static_cast<uint8_t>(samplesToSend - 1);
    header.nuFrame = frameCounter++;

    // Copy header
    std::memcpy(packetData, &header, HEADER_SIZE);

    // Convert and interleave audio data
    uint8_t* audioData = packetData + HEADER_SIZE;

    if (dataType == DataType::INT16) {
        int16_t* dst = reinterpret_cast<int16_t*>(audioData);
        for (int s = 0; s < samplesToSend; ++s) {
            for (int ch = 0; ch < channels; ++ch) {
                *dst++ = floatToInt16(channelData[ch][s]);
            }
        }
    } else if (dataType == DataType::FLOAT32) {
        float* dst = reinterpret_cast<float*>(audioData);
        for (int s = 0; s < samplesToSend; ++s) {
            for (int ch = 0; ch < channels; ++ch) {
                *dst++ = channelData[ch][s];
            }
        }
    }

    return static_cast<int>(HEADER_SIZE + samplesToSend * channels * sampleSize);
}

int Packet::deserialize(const uint8_t* packetData, size_t packetSize,
                        float* interleavedOutput, int maxSamples, int& outNumChannels) {

    if (packetSize < HEADER_SIZE) return 0;

    Header packetHeader;
    std::memcpy(&packetHeader, packetData, HEADER_SIZE);

    if (packetHeader.magic != MAGIC) return 0;

    int numChannels = packetHeader.format_nbc + 1;
    int numSamples = packetHeader.format_nbs + 1;
    DataType packetDataType = static_cast<DataType>(packetHeader.format_bit & DATATYPE_MASK);
    int sampleSize = getSampleSize(packetDataType);

    size_t expectedDataSize = numSamples * numChannels * sampleSize;
    if (packetSize < HEADER_SIZE + expectedDataSize) return 0;

    int samplesToRead = std::min(numSamples, maxSamples);
    outNumChannels = numChannels;

    const uint8_t* audioData = packetData + HEADER_SIZE;

    if (packetDataType == DataType::INT16) {
        const int16_t* src = reinterpret_cast<const int16_t*>(audioData);
        for (int s = 0; s < samplesToRead; ++s) {
            for (int ch = 0; ch < numChannels; ++ch) {
                interleavedOutput[s * numChannels + ch] = int16ToFloat(*src++);
            }
        }
    } else if (packetDataType == DataType::FLOAT32) {
        const float* src = reinterpret_cast<const float*>(audioData);
        std::memcpy(interleavedOutput, src, samplesToRead * numChannels * sizeof(float));
    } else {
        // Unsupported format, fill with zeros
        std::memset(interleavedOutput, 0, samplesToRead * numChannels * sizeof(float));
    }

    return samplesToRead;
}

uint32_t Packet::getAndIncrementFrameCounter() {
    return frameCounter++;
}

bool Packet::parseHeader(const uint8_t* packetData, size_t packetSize, Header& outHeader) {
    if (packetSize < HEADER_SIZE) return false;

    std::memcpy(&outHeader, packetData, HEADER_SIZE);
    return outHeader.magic == MAGIC;
}

int Packet::getMaxSamplesPerPacket(int numChannels, DataType dataType) {
    int sampleSize = getSampleSize(dataType);
    return static_cast<int>(MAX_DATA_SIZE / (numChannels * sampleSize));
}

int16_t Packet::floatToInt16(float sample) {
    // Clamp and convert
    float clamped = std::clamp(sample, -1.0f, 1.0f);
    return static_cast<int16_t>(clamped * 32767.0f);
}

float Packet::int16ToFloat(int16_t sample) {
    return static_cast<float>(sample) / 32767.0f;
}

} // namespace vban
