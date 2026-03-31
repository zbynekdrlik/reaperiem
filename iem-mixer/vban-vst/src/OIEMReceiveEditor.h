#pragma once
#include <JuceHeader.h>
#include "OIEMReceiveProcessor.h"
#include "ui/MeterComponent.h"

class OIEMReceiveEditor : public juce::AudioProcessorEditor,
                          public juce::Timer {
public:
    explicit OIEMReceiveEditor(OIEMReceiveProcessor& processor);
    ~OIEMReceiveEditor() override;

    void paint(juce::Graphics& g) override;
    void resized() override;
    void timerCallback() override;

private:
    OIEMReceiveProcessor& processor;

    juce::Label titleLabel{"", "OIEM Receive"};
    juce::Label infoLabel{"", "Talkback"};
    juce::Label portLabel{"", "Listening on ephemeral port"};
    MeterComponent meterComponent;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(OIEMReceiveEditor)
};
