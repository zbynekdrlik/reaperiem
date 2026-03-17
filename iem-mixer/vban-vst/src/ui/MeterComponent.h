#pragma once
#include <JuceHeader.h>

class MeterComponent : public juce::Component, public juce::Timer {
public:
    MeterComponent();

    void setLevels(float input, float output);
    void setStatus(bool connected, uint64_t packets, bool isSendMode, bool isActive);

    void paint(juce::Graphics& g) override;
    void resized() override;
    void timerCallback() override;

private:
    std::atomic<float> inputLevel{0.0f};
    std::atomic<float> outputLevel{0.0f};
    std::atomic<bool> connected{false};
    std::atomic<uint64_t> packetCount{0};
    std::atomic<bool> sendMode{true};
    std::atomic<bool> active{false};

    float displayInputLevel = 0.0f;
    float displayOutputLevel = 0.0f;

    void drawMeter(juce::Graphics& g, juce::Rectangle<int> bounds, float level, const juce::String& label);

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(MeterComponent)
};
