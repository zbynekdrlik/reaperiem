#include "OIEMReceiveProcessor.h"
#include "OIEMReceiveEditor.h"

OIEMReceiveProcessor::OIEMReceiveProcessor()
    : AudioProcessor(BusesProperties()
        .withOutput("Output", juce::AudioChannelSet::stereo(), true))
{
}

OIEMReceiveProcessor::~OIEMReceiveProcessor() {
    opusReceiver.stop();
}

bool OIEMReceiveProcessor::isBusesLayoutSupported(const BusesLayout& layouts) const {
    const auto& mainOutput = layouts.getMainOutputChannelSet();

    if (mainOutput.isDisabled())
        return false;

    // Output-only plugin: accept stereo or mono output
    int channels = mainOutput.size();
    return channels >= 1 && channels <= 2;
}

void OIEMReceiveProcessor::prepareToPlay(double sampleRate, int samplesPerBlock) {
    currentSampleRate = sampleRate;
    currentBlockSize = samplesPerBlock;
    numChannels = juce::jmin(getTotalNumOutputChannels(), 2);

    // Resample ratio: input rate (48kHz from Opus) / output rate (host)
    // e.g., 48000/96000 = 0.5 (need 0.5 input samples per output sample,
    // i.e., upsample by 2x)
    resampleRatio = 48000.0 / sampleRate;

    // Initialize per-channel LagrangeInterpolators
    interpolators.clear();
    interpolators.resize(numChannels);
    for (auto& interp : interpolators) {
        interp.reset();
    }

    // Allocate decode buffers
    decodedInterleaved.resize(OPUS_FRAME_SAMPLES * 2); // stereo interleaved
    decoded48kLeft.resize(OPUS_FRAME_SAMPLES);
    decoded48kRight.resize(OPUS_FRAME_SAMPLES);

    // Accumulation buffers — hold decoded 48kHz samples between processBlock calls
    accumLeft.clear();
    accumRight.clear();
    accumLeft.reserve(OPUS_FRAME_SAMPLES * 4);
    accumRight.reserve(OPUS_FRAME_SAMPLES * 4);

    // Resampled output buffers at host sample rate
    resampledLeft.resize(samplesPerBlock + 16);
    resampledRight.resize(samplesPerBlock + 16);

    // Opus frame scratch buffer
    opusFrameBuffer.resize(2048);

    // Configure Opus decoder for 48kHz stereo
    opusDecoder.configure(48000, 2);

    // Configure and start UDP receiver
    opusReceiver.configure(DEST_IP, PORT);
    opusReceiver.start();

    setLatencySamples(samplesPerBlock);
}

void OIEMReceiveProcessor::releaseResources() {
    opusReceiver.stop();
    accumLeft.clear();
    accumRight.clear();
}

void OIEMReceiveProcessor::processBlock(juce::AudioBuffer<float>& buffer, juce::MidiBuffer&) {
    juce::ScopedNoDenormals noDenormals;

    int numSamples = buffer.getNumSamples();
    int channels = juce::jmin(buffer.getNumChannels(), numChannels);

    // Pop all available Opus frames from receiver and decode them
    while (true) {
        int frameSize = opusReceiver.popFrame(opusFrameBuffer.data(),
                                               static_cast<int>(opusFrameBuffer.size()));
        if (frameSize <= 0)
            break;

        // Decode Opus frame to interleaved 48kHz float PCM
        int samplesDecoded = opusDecoder.decode(opusFrameBuffer.data(), frameSize,
                                                 decodedInterleaved.data(),
                                                 OPUS_FRAME_SAMPLES);
        if (samplesDecoded <= 0)
            continue;

        // Deinterleave into per-channel accumulation buffers
        for (int i = 0; i < samplesDecoded; ++i) {
            accumLeft.push_back(decodedInterleaved[i * 2]);
            accumRight.push_back(decodedInterleaved[i * 2 + 1]);
        }
    }

    // Calculate how many 48kHz input samples we need for this block
    // numSamples output samples * resampleRatio = input samples needed
    int inputSamplesNeeded = static_cast<int>(numSamples * resampleRatio) + 4;

    if (static_cast<int>(accumLeft.size()) >= inputSamplesNeeded && inputSamplesNeeded > 0) {
        // Resample from 48kHz to host sample rate
        if (channels >= 1) {
            interpolators[0].process(
                resampleRatio,
                accumLeft.data(),
                resampledLeft.data(),
                numSamples,
                inputSamplesNeeded,
                0);
        }

        if (channels >= 2) {
            interpolators[1].process(
                resampleRatio,
                accumRight.data(),
                resampledRight.data(),
                numSamples,
                inputSamplesNeeded,
                0);
        }

        // Copy resampled audio to output buffer
        if (channels >= 1) {
            std::memcpy(buffer.getWritePointer(0), resampledLeft.data(),
                        sizeof(float) * numSamples);
        }
        if (channels >= 2) {
            std::memcpy(buffer.getWritePointer(1), resampledRight.data(),
                        sizeof(float) * numSamples);
        }

        // Remove consumed input samples from accumulation buffers
        int consumed = static_cast<int>(numSamples * resampleRatio);
        if (consumed > static_cast<int>(accumLeft.size()))
            consumed = static_cast<int>(accumLeft.size());

        accumLeft.erase(accumLeft.begin(), accumLeft.begin() + consumed);
        accumRight.erase(accumRight.begin(), accumRight.begin() + consumed);

        // Calculate output level
        float maxLevel = 0.0f;
        for (int ch = 0; ch < channels; ++ch) {
            auto* data = buffer.getReadPointer(ch);
            for (int s = 0; s < numSamples; ++s) {
                maxLevel = std::max(maxLevel, std::abs(data[s]));
            }
        }
        outputLevel = maxLevel;
    } else {
        // Not enough decoded samples — output silence
        buffer.clear();
        outputLevel = 0.0f;
    }
}

void OIEMReceiveProcessor::getStateInformation(juce::MemoryBlock&) {
    // No state to save — plugin is always active
}

void OIEMReceiveProcessor::setStateInformation(const void*, int) {
    // No state to restore — plugin is always active
}

juce::AudioProcessorEditor* OIEMReceiveProcessor::createEditor() {
    return new OIEMReceiveEditor(*this);
}

bool OIEMReceiveProcessor::isReceiving() const {
    return opusReceiver.isReceiving();
}

uint64_t OIEMReceiveProcessor::getPacketCount() const {
    return opusReceiver.getPacketsReceived();
}

// Plugin instantiation
juce::AudioProcessor* JUCE_CALLTYPE createPluginFilter() {
    return new OIEMReceiveProcessor();
}
