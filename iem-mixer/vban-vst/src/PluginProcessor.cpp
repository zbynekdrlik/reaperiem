#include "PluginProcessor.h"
#include "PluginEditor.h"

VBANIEMProcessor::VBANIEMProcessor()
    : AudioProcessor(BusesProperties()
        .withInput("Input", juce::AudioChannelSet::stereo(), true)
        .withOutput("Output", juce::AudioChannelSet::stereo(), true))
{
}

VBANIEMProcessor::~VBANIEMProcessor() {
    opusSender.stop();
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
    numChannels = juce::jmin(getTotalNumInputChannels(), 2); // Opus supports max stereo

    // Calculate resample ratio: how many input samples per output sample
    // e.g., 96000/48000 = 2.0 (we need 2 input samples to produce 1 output sample)
    resampleRatio = sampleRate / 48000.0;

    // Initialize per-channel LagrangeInterpolators
    interpolators.clear();
    interpolators.resize(numChannels);
    for (auto& interp : interpolators) {
        interp.reset();
    }

    // Pre-allocate resampling buffers
    // Max output samples = ceil(blockSize / ratio) + some margin
    int maxOutputSamples = static_cast<int>(std::ceil(samplesPerBlock / resampleRatio)) + 16;
    resampledLeft.resize(maxOutputSamples);
    resampledRight.resize(maxOutputSamples);
    interleavedBuffer.resize(maxOutputSamples * numChannels);

    // Configure Opus encoder for 48kHz stereo
    opusEncoder.configure(48000, numChannels, 160000);

    // Configure and start UDP sender
    opusSender.configure(DEST_IP, PORT);
    opusSender.start();

    // Report minimal latency — LagrangeInterpolator has zero group delay
    setLatencySamples(samplesPerBlock);
}

void VBANIEMProcessor::releaseResources() {
    opusSender.stop();
}

void VBANIEMProcessor::processBlock(juce::AudioBuffer<float>& buffer, juce::MidiBuffer&) {
    juce::ScopedNoDenormals noDenormals;

    int numSamples = buffer.getNumSamples();
    int channels = juce::jmin(buffer.getNumChannels(), numChannels);

    // Calculate input level
    float maxLevel = 0.0f;
    for (int ch = 0; ch < channels; ++ch) {
        auto* data = buffer.getReadPointer(ch);
        for (int s = 0; s < numSamples; ++s) {
            maxLevel = std::max(maxLevel, std::abs(data[s]));
        }
    }
    inputLevel = maxLevel;

    // Resample each channel from currentSampleRate to 48kHz
    // LagrangeInterpolator::process: ratio = inputRate/outputRate
    // Returns number of output samples actually produced
    int outputSamples = 0;

    if (channels >= 1 && numSamples > 0) {
        int maxOut = static_cast<int>(resampledLeft.size());
        outputSamples = interpolators[0].process(
            resampleRatio,
            buffer.getReadPointer(0),
            resampledLeft.data(),
            maxOut);
    }

    if (channels >= 2 && numSamples > 0) {
        int maxOut = static_cast<int>(resampledRight.size());
        int rightOut = interpolators[1].process(
            resampleRatio,
            buffer.getReadPointer(1),
            resampledRight.data(),
            maxOut);
        // Use minimum to stay safe
        outputSamples = juce::jmin(outputSamples, rightOut);
    }

    if (outputSamples > 0) {
        // Interleave the resampled channels
        for (int i = 0; i < outputSamples; ++i) {
            interleavedBuffer[i * channels] = resampledLeft[i];
            if (channels >= 2)
                interleavedBuffer[i * channels + 1] = resampledRight[i];
        }

        // Encode to Opus and push frames to sender
        std::vector<std::vector<uint8_t>> opusFrames;
        opusEncoder.encode(interleavedBuffer.data(), outputSamples, opusFrames);

        for (const auto& frame : opusFrames) {
            opusSender.pushFrame(frame.data(), static_cast<int>(frame.size()));
        }
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
    return opusSender.isSending();
}

uint64_t VBANIEMProcessor::getPacketCount() const {
    return opusSender.getPacketsSent();
}

// Plugin instantiation
juce::AudioProcessor* JUCE_CALLTYPE createPluginFilter() {
    return new VBANIEMProcessor();
}
