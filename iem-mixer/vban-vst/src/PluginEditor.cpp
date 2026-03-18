#include "PluginEditor.h"

VBANIEMEditor::VBANIEMEditor(VBANIEMProcessor& p)
    : AudioProcessorEditor(p)
    , processor(p)
{
    // Title
    titleLabel.setFont(juce::Font(24.0f, juce::Font::bold));
    titleLabel.setColour(juce::Label::textColourId, juce::Colours::white);
    titleLabel.setJustificationType(juce::Justification::centred);
    addAndMakeVisible(titleLabel);

    // Destination info
    destLabel.setFont(juce::Font(13.0f));
    destLabel.setColour(juce::Label::textColourId, juce::Colours::lightgrey);
    destLabel.setJustificationType(juce::Justification::centred);
    addAndMakeVisible(destLabel);

    // Stream name info
    streamLabel.setFont(juce::Font(13.0f));
    streamLabel.setColour(juce::Label::textColourId, juce::Colours::lightgrey);
    streamLabel.setJustificationType(juce::Justification::centred);
    addAndMakeVisible(streamLabel);

    // Meter component
    addAndMakeVisible(meterComponent);

    // Start UI update timer
    startTimerHz(30);

    setSize(250, 320);
}

VBANIEMEditor::~VBANIEMEditor() {
    stopTimer();
}

void VBANIEMEditor::paint(juce::Graphics& g) {
    g.fillAll(juce::Colour(0xff202020));
}

void VBANIEMEditor::resized() {
    auto bounds = getLocalBounds();

    titleLabel.setBounds(bounds.removeFromTop(40));
    destLabel.setBounds(bounds.removeFromTop(20));
    streamLabel.setBounds(bounds.removeFromTop(20));

    // Meters take the rest
    meterComponent.setBounds(bounds);
}

void VBANIEMEditor::timerCallback() {
    // Update meters
    meterComponent.setLevels(processor.getInputLevel(), processor.getOutputLevel());
    meterComponent.setStatus(
        processor.isConnected(),
        processor.getPacketCount(),
        true,  // always send mode
        true   // always active
    );
}
