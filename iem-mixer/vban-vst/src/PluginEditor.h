#pragma once
#include <JuceHeader.h>
#include "PluginProcessor.h"
#include "ui/MeterComponent.h"

class VBANIEMEditor : public juce::AudioProcessorEditor,
                      public juce::Timer {
public:
    explicit VBANIEMEditor(VBANIEMProcessor& processor);
    ~VBANIEMEditor() override;

    void paint(juce::Graphics& g) override;
    void resized() override;
    void timerCallback() override;

private:
    VBANIEMProcessor& processor;

    juce::Label titleLabel{"", "VBAN IEM"};
    juce::Label destLabel{"", "Sending to 127.0.0.1:6980"};
    juce::Label streamLabel{"", "Stream: engineer"};
    juce::ToggleButton activeButton{"Active"};
    MeterComponent meterComponent;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(VBANIEMEditor)
};
