#pragma once
#include <JuceHeader.h>

class AudioRingBuffer {
public:
    AudioRingBuffer(int numChannels, int numSamples);

    void setSize(int numChannels, int numSamples);
    void reset();

    int getNumSamplesAvailableForWriting() const;
    int getNumSamplesAvailableForReading() const;

    void writeSamples(const float* const* channelData, int numChannels, int numSamples);
    void readSamples(float* const* channelData, int numChannels, int numSamples);

    // Interleaved versions for network data
    void writeInterleavedSamples(const float* interleavedData, int numChannels, int numSamples);
    void readInterleavedSamples(float* interleavedData, int numChannels, int numSamples);

private:
    juce::AbstractFifo fifo;
    juce::AudioBuffer<float> buffer;
    int channelCount;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(AudioRingBuffer)
};
