#include "MeterComponent.h"
#include <cmath>

MeterComponent::MeterComponent() {
    startTimerHz(30); // 30 FPS refresh
}

void MeterComponent::setLevels(float input, float output) {
    inputLevel = input;
    outputLevel = output;
}

void MeterComponent::setStatus(bool isConnected, uint64_t packets, bool isSendMode, bool isActive) {
    connected = isConnected;
    packetCount = packets;
    sendMode = isSendMode;
    active = isActive;
}

void MeterComponent::timerCallback() {
    // Smooth level decay
    float targetInput = inputLevel.load();
    float targetOutput = outputLevel.load();

    displayInputLevel = displayInputLevel * 0.8f + targetInput * 0.2f;
    displayOutputLevel = displayOutputLevel * 0.8f + targetOutput * 0.2f;

    repaint();
}

void MeterComponent::paint(juce::Graphics& g) {
    g.fillAll(juce::Colour(0xff1a1a1a));

    auto bounds = getLocalBounds().reduced(10);

    bool isActive_ = active.load();
    bool isConnected = connected.load();

    // Mode indicator
    auto modeArea = bounds.removeFromTop(24);
    g.setColour(juce::Colours::darkgreen);
    g.fillRoundedRectangle(modeArea.toFloat(), 4.0f);
    g.setColour(juce::Colours::white);
    g.setFont(14.0f);
    g.drawText("VBAN IEM SENDER", modeArea, juce::Justification::centred);

    bounds.removeFromTop(8);

    // Status indicator
    auto statusArea = bounds.removeFromTop(24);

    // Determine status text and color
    juce::String statusText;
    juce::Colour statusColor;

    if (!isActive_) {
        statusText = "Stopped";
        statusColor = juce::Colours::darkgrey;
    } else if (isConnected) {
        statusText = "Sending to 127.0.0.1:6980";
        statusColor = juce::Colours::green;
    } else {
        statusText = "Waiting for audio";
        statusColor = juce::Colours::orange;
    }

    g.setColour(statusColor);
    g.fillEllipse(statusArea.removeFromLeft(20).toFloat().reduced(4));

    g.setColour(juce::Colours::white);
    g.setFont(13.0f);
    g.drawText(statusText, statusArea.removeFromLeft(180), juce::Justification::centredLeft);

    g.setColour(juce::Colours::lightgrey);
    g.setFont(11.0f);
    g.drawText("Pkts: " + juce::String(packetCount.load()), statusArea, juce::Justification::centredRight);

    bounds.removeFromTop(8);

    // Meters
    int meterWidth = (bounds.getWidth() - 20) / 2;

    auto inputBounds = bounds.removeFromLeft(meterWidth);
    bounds.removeFromLeft(20);
    auto outputBounds = bounds;

    drawMeter(g, inputBounds, displayInputLevel, "INPUT");
    drawMeter(g, outputBounds, displayOutputLevel, "THRU");
}

void MeterComponent::drawMeter(juce::Graphics& g, juce::Rectangle<int> bounds, float level, const juce::String& label) {
    // Label
    g.setColour(juce::Colours::white);
    g.setFont(11.0f);
    g.drawText(label, bounds.removeFromTop(18), juce::Justification::centred);

    // Meter background
    g.setColour(juce::Colour(0xff333333));
    g.fillRoundedRectangle(bounds.toFloat(), 4.0f);

    // Meter fill — convert linear level to dB-scaled meter position to match REAPER's display
    float levelDb = (level > 0.0001f) ? 20.0f * std::log10f(level) : -80.0f;
    // Map dB range [-60, 0] to [0.0, 1.0] for meter fill
    float meterPct = std::clamp((levelDb + 60.0f) / 60.0f, 0.0f, 1.0f);
    float meterHeight = bounds.getHeight() * meterPct;
    auto fillBounds = bounds.removeFromBottom(static_cast<int>(meterHeight));

    // Gradient from green to yellow to red
    juce::ColourGradient gradient;
    if (level < 0.7f) {
        gradient = juce::ColourGradient(
            juce::Colours::green, bounds.getX(), bounds.getBottom(),
            juce::Colours::yellow, bounds.getX(), bounds.getY(), false);
    } else {
        gradient = juce::ColourGradient(
            juce::Colours::yellow, bounds.getX(), bounds.getBottom(),
            juce::Colours::red, bounds.getX(), bounds.getY(), false);
    }

    g.setGradientFill(gradient);
    g.fillRoundedRectangle(fillBounds.toFloat(), 4.0f);

    // dB scale marks — placed using the same dB-to-percent mapping as the meter fill
    g.setColour(juce::Colours::grey.withAlpha(0.5f));
    int totalHeight = bounds.getHeight() + fillBounds.getHeight();
    for (float db : {-6.0f, -12.0f, -24.0f}) {
        float markPct = (db + 60.0f) / 60.0f;
        int y = bounds.getBottom() - static_cast<int>(totalHeight * markPct);
        g.drawHorizontalLine(y, bounds.getX() + 2.0f, bounds.getRight() - 2.0f);
    }
}

void MeterComponent::resized() {
    // Layout handled in paint()
}
