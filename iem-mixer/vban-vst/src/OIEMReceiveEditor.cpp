#include "OIEMReceiveEditor.h"

OIEMReceiveEditor::OIEMReceiveEditor(OIEMReceiveProcessor& p)
    : AudioProcessorEditor(p)
    , processor(p)
{
    // Title
    titleLabel.setFont(juce::Font(24.0f, juce::Font::bold));
    titleLabel.setColour(juce::Label::textColourId, juce::Colours::white);
    titleLabel.setJustificationType(juce::Justification::centred);
    addAndMakeVisible(titleLabel);

    // Info label
    infoLabel.setFont(juce::Font(14.0f));
    infoLabel.setColour(juce::Label::textColourId, juce::Colours::lightgreen);
    infoLabel.setJustificationType(juce::Justification::centred);
    addAndMakeVisible(infoLabel);

    // Port info
    portLabel.setFont(juce::Font(13.0f));
    portLabel.setColour(juce::Label::textColourId, juce::Colours::lightgrey);
    portLabel.setJustificationType(juce::Justification::centred);
    addAndMakeVisible(portLabel);

    // Meter component
    addAndMakeVisible(meterComponent);

    // Start UI update timer
    startTimerHz(30);

    setSize(250, 320);
}

OIEMReceiveEditor::~OIEMReceiveEditor() {
    stopTimer();
}

void OIEMReceiveEditor::paint(juce::Graphics& g) {
    g.fillAll(juce::Colour(0xff202020));
}

void OIEMReceiveEditor::resized() {
    auto bounds = getLocalBounds();

    titleLabel.setBounds(bounds.removeFromTop(40));
    infoLabel.setBounds(bounds.removeFromTop(20));
    portLabel.setBounds(bounds.removeFromTop(20));

    // Meters take the rest
    meterComponent.setBounds(bounds);
}

void OIEMReceiveEditor::timerCallback() {
    // Update port label with actual bound port
    auto port = processor.getPacketCount() > 0
        ? "Heartbeat -> 127.0.0.1:7980"
        : "Waiting for audio...";
    portLabel.setText(port, juce::dontSendNotification);

    // Update meters — receive mode: no input level, only output
    meterComponent.setLevels(0.0f, processor.getOutputLevel());
    meterComponent.setStatus(
        processor.isReceiving(),
        processor.getPacketCount(),
        false,  // receive mode (not send)
        true    // always active
    );
}
