#pragma once
#include <JuceHeader.h>
#include <opus.h>
#include <vector>
#include <cstdint>

/// Wraps libopus C API for decoding Opus frames to 48kHz float PCM.
/// Mirror of OpusEncoderWrapper, but for the receive direction.
class OpusDecoderWrapper {
public:
    OpusDecoderWrapper();
    ~OpusDecoderWrapper();

    /// Initialize the decoder. Returns true on success.
    bool configure(int sampleRate = 48000, int channels = 2);

    /// Decode a single Opus packet to interleaved float PCM.
    /// Returns the number of samples per channel decoded, or 0 on error.
    /// Output is written to `outPCM` which must have space for
    /// at least MAX_FRAME_SAMPLES * channels floats.
    int decode(const uint8_t* opusData, int opusSize,
               float* outPCM, int maxSamplesPerChannel);

    bool isInitialized() const { return decoder != nullptr; }
    int getNumChannels() const { return numChannels; }

private:
    static constexpr int MAX_FRAME_SAMPLES = 960; // 20ms @ 48kHz

    OpusDecoder* decoder = nullptr;
    int numChannels = 2;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(OpusDecoderWrapper)
};
