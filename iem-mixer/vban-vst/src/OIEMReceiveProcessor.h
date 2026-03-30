#pragma once
#include <JuceHeader.h>
#include "opus/OpusDecoderWrapper.h"
#include "opus/OpusReceiver.h"

class OIEMReceiveProcessor : public juce::AudioProcessor {
public:
    OIEMReceiveProcessor();
    ~OIEMReceiveProcessor() override;

    // AudioProcessor interface
    void prepareToPlay(double sampleRate, int samplesPerBlock) override;
    void releaseResources() override;
    void processBlock(juce::AudioBuffer<float>&, juce::MidiBuffer&) override;

    bool isBusesLayoutSupported(const BusesLayout& layouts) const override;

    // Plugin info
    const juce::String getName() const override { return "OIEM Receive"; }
    bool acceptsMidi() const override { return false; }
    bool producesMidi() const override { return false; }
    bool isMidiEffect() const override { return false; }
    double getTailLengthSeconds() const override { return 0.0; }

    // Programs
    int getNumPrograms() override { return 1; }
    int getCurrentProgram() override { return 0; }
    void setCurrentProgram(int) override {}
    const juce::String getProgramName(int) override { return {}; }
    void changeProgramName(int, const juce::String&) override {}

    // State
    void getStateInformation(juce::MemoryBlock& destData) override;
    void setStateInformation(const void* data, int sizeInBytes) override;

    // Editor
    bool hasEditor() const override { return true; }
    juce::AudioProcessorEditor* createEditor() override;

    // Status
    bool isReceiving() const;
    uint64_t getPacketCount() const;

    // Level monitoring
    float getOutputLevel() const { return outputLevel.load(); }

private:
    // Hardcoded configuration for IEM talkback use case
    static constexpr const char* DEST_IP = "127.0.0.1";
    static constexpr uint16_t PORT = 7980;

    // Opus decoder (48kHz stereo)
    OpusDecoderWrapper opusDecoder;
    // UDP receiver thread
    OpusReceiver opusReceiver;

    // JUCE LagrangeInterpolator for resampling from 48kHz to host sample rate
    std::vector<juce::LagrangeInterpolator> interpolators;

    // Decode buffer: interleaved 48kHz PCM from Opus decoder
    static constexpr int OPUS_FRAME_SAMPLES = 960; // 20ms @ 48kHz
    std::vector<float> decodedInterleaved;

    // Per-channel buffers at 48kHz (deinterleaved)
    std::vector<float> decoded48kLeft;
    std::vector<float> decoded48kRight;

    // Accumulation buffers for 48kHz samples (between processBlock calls)
    std::vector<float> accumLeft;
    std::vector<float> accumRight;

    // Resampled output buffers at host sample rate
    std::vector<float> resampledLeft;
    std::vector<float> resampledRight;

    // Scratch buffer for popping Opus frames
    std::vector<uint8_t> opusFrameBuffer;

    double currentSampleRate = 96000.0;
    int currentBlockSize = 512;
    int numChannels = 2;
    double resampleRatio = 1.0; // 48000 / hostSampleRate (e.g., 48000/96000 = 0.5)

    std::atomic<float> outputLevel{0.0f};

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(OIEMReceiveProcessor)
};
