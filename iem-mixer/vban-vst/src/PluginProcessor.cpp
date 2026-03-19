#include "PluginProcessor.h"
#include "PluginEditor.h"

VBANIEMProcessor::VBANIEMProcessor()
    : AudioProcessor(BusesProperties()
        .withInput("Input", juce::AudioChannelSet::stereo(), true)
        .withOutput("Output", juce::AudioChannelSet::stereo(), true))
    , sender(std::make_unique<VBANSender>())
{
}

VBANIEMProcessor::~VBANIEMProcessor() {
    sender->stop();
}

bool VBANIEMProcessor::isBusesLayoutSupported(const BusesLayout& layouts) const {
    const auto& mainInput = layouts.getMainInputChannelSet();
    const auto& mainOutput = layouts.getMainOutputChannelSet();

    if (mainInput != mainOutput)
        return false;

    if (mainInput.isDisabled())
        return false;

    int channels = mainInput.size();
    return channels >= 1 && channels <= 8;
}

void VBANIEMProcessor::prepareToPlay(double sampleRate, int samplesPerBlock) {
    currentSampleRate = sampleRate;
    currentBlockSize = samplesPerBlock;
    numChannels = getTotalNumInputChannels();

    // Configure sender with hardcoded IEM settings
    sender->configure(DEST_IP, PORT, STREAM_NAME,
                      static_cast<int>(sampleRate), numChannels);

    int latencySamples = samplesPerBlock * 2;
    setLatencySamples(latencySamples);

    // Always start sending immediately
    sender->start();
}

void VBANIEMProcessor::releaseResources() {
    sender->stop();
}

void VBANIEMProcessor::processBlock(juce::AudioBuffer<float>& buffer, juce::MidiBuffer&) {
    juce::ScopedNoDenormals noDenormals;

    int numSamples = buffer.getNumSamples();
    int channels = buffer.getNumChannels();

    // Calculate input level
    float maxLevel = 0.0f;
    for (int ch = 0; ch < channels; ++ch) {
        auto* data = buffer.getReadPointer(ch);
        for (int s = 0; s < numSamples; ++s) {
            maxLevel = std::max(maxLevel, std::abs(data[s]));
        }
    }
    inputLevel = maxLevel;

    // Always send audio via VBAN
    if (sender->isRunning()) {
        sender->pushSamples(buffer.getArrayOfReadPointers(), numSamples);
    }

    // Pass through audio unchanged
    outputLevel = inputLevel.load();
}

void VBANIEMProcessor::getStateInformation(juce::MemoryBlock&) {
    // No state to save — plugin is always active
}

void VBANIEMProcessor::setStateInformation(const void*, int) {
    // No state to restore — plugin is always active
}

juce::AudioProcessorEditor* VBANIEMProcessor::createEditor() {
    return new VBANIEMEditor(*this);
}

bool VBANIEMProcessor::isConnected() const {
    return sender->isSending();
}

uint64_t VBANIEMProcessor::getPacketCount() const {
    return sender->getPacketsSent();
}

// Plugin instantiation
juce::AudioProcessor* JUCE_CALLTYPE createPluginFilter() {
    return new VBANIEMProcessor();
}
