#pragma once
#include <JuceHeader.h>
#include <opus.h>
#include <vector>
#include <cstdint>

/// Wraps libopus C API for encoding 48kHz stereo audio to Opus frames.
/// Accumulates input samples until a full 20ms frame (960 samples/ch) is ready.
class OpusEncoderWrapper {
public:
    OpusEncoderWrapper();
    ~OpusEncoderWrapper();

    /// Initialize the encoder. Returns true on success.
    bool configure(int sampleRate = 48000, int channels = 2,
                   int bitrate = 160000);

    /// Feed interleaved float samples (already resampled to 48kHz).
    /// When enough samples accumulate (960 per channel = 20ms),
    /// encoded Opus frames are appended to `outFrames`.
    /// Each entry in outFrames is a single Opus packet.
    void encode(const float* interleaved, int numSamplesPerChannel,
                std::vector<std::vector<uint8_t>>& outFrames);

    bool isInitialized() const { return encoder != nullptr; }

private:
    static constexpr int OPUS_FRAME_SAMPLES = 960; // 20ms @ 48kHz

    OpusEncoder* encoder = nullptr;
    int numChannels = 2;

    /// Accumulation buffer for interleaved samples
    std::vector<float> accumBuffer;

    /// Scratch buffer for encoded output
    std::vector<uint8_t> encodedBuffer;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(OpusEncoderWrapper)
};
