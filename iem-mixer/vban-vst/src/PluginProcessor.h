#pragma once
#include <JuceHeader.h>
#include "vban/VBANSender.h"

class VBANIEMProcessor : public juce::AudioProcessor {
public:
    VBANIEMProcessor();
    ~VBANIEMProcessor() override;

    // AudioProcessor interface
    void prepareToPlay(double sampleRate, int samplesPerBlock) override;
    void releaseResources() override;
    void processBlock(juce::AudioBuffer<float>&, juce::MidiBuffer&) override;

    bool isBusesLayoutSupported(const BusesLayout& layouts) const override;

    // Plugin info
    const juce::String getName() const override { return "VBAN IEM"; }
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

    // Configuration
    void setActive(bool active);
    bool isActive() const { return active; }

    // Status
    bool isConnected() const;
    uint64_t getPacketCount() const;

    // Level monitoring
    float getInputLevel() const { return inputLevel.load(); }
    float getOutputLevel() const { return outputLevel.load(); }

private:
    // Hardcoded configuration for IEM use case
    static constexpr const char* DEST_IP = "127.0.0.1";
    static constexpr uint16_t PORT = 6980;
    static constexpr const char* STREAM_NAME = "engineer";

    bool active = false;

    std::unique_ptr<VBANSender> sender;

    double currentSampleRate = 48000.0;
    int currentBlockSize = 512;
    int numChannels = 2;

    std::atomic<float> inputLevel{0.0f};
    std::atomic<float> outputLevel{0.0f};

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(VBANIEMProcessor)
};
