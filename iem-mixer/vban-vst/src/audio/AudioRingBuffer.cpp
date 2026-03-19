#include "AudioRingBuffer.h"

AudioRingBuffer::AudioRingBuffer(int numChannels, int numSamples)
    : fifo(numSamples)
    , buffer(numChannels, numSamples)
    , channelCount(numChannels)
{
    buffer.clear();
}

void AudioRingBuffer::setSize(int numChannels, int numSamples) {
    fifo.setTotalSize(numSamples);
    buffer.setSize(numChannels, numSamples, false, true, false);
    channelCount = numChannels;
    reset();
}

void AudioRingBuffer::reset() {
    fifo.reset();
    buffer.clear();
}

int AudioRingBuffer::getNumSamplesAvailableForWriting() const {
    return fifo.getFreeSpace();
}

int AudioRingBuffer::getNumSamplesAvailableForReading() const {
    return fifo.getNumReady();
}

void AudioRingBuffer::writeSamples(const float* const* channelData, int numChannels, int numSamples) {
    int start1, size1, start2, size2;
    fifo.prepareToWrite(numSamples, start1, size1, start2, size2);

    int channels = juce::jmin(numChannels, channelCount);

    if (size1 > 0) {
        for (int ch = 0; ch < channels; ++ch) {
            buffer.copyFrom(ch, start1, channelData[ch], size1);
        }
    }

    if (size2 > 0) {
        for (int ch = 0; ch < channels; ++ch) {
            buffer.copyFrom(ch, start2, channelData[ch] + size1, size2);
        }
    }

    fifo.finishedWrite(size1 + size2);
}

void AudioRingBuffer::readSamples(float* const* channelData, int numChannels, int numSamples) {
    int start1, size1, start2, size2;
    fifo.prepareToRead(numSamples, start1, size1, start2, size2);

    int channels = juce::jmin(numChannels, channelCount);
    int totalRead = size1 + size2;

    if (size1 > 0) {
        for (int ch = 0; ch < channels; ++ch) {
            juce::FloatVectorOperations::copy(channelData[ch], buffer.getReadPointer(ch, start1), size1);
        }
    }

    if (size2 > 0) {
        for (int ch = 0; ch < channels; ++ch) {
            juce::FloatVectorOperations::copy(channelData[ch] + size1, buffer.getReadPointer(ch, start2), size2);
        }
    }

    // Zero-fill if we didn't have enough samples
    if (totalRead < numSamples) {
        for (int ch = 0; ch < channels; ++ch) {
            juce::FloatVectorOperations::clear(channelData[ch] + totalRead, numSamples - totalRead);
        }
    }

    fifo.finishedRead(totalRead);
}

void AudioRingBuffer::writeInterleavedSamples(const float* interleavedData, int numChannels, int numSamples) {
    int start1, size1, start2, size2;
    fifo.prepareToWrite(numSamples, start1, size1, start2, size2);

    int channels = juce::jmin(numChannels, channelCount);

    // Deinterleave and write
    if (size1 > 0) {
        for (int s = 0; s < size1; ++s) {
            for (int ch = 0; ch < channels; ++ch) {
                buffer.setSample(ch, start1 + s, interleavedData[s * numChannels + ch]);
            }
        }
    }

    if (size2 > 0) {
        const float* src = interleavedData + size1 * numChannels;
        for (int s = 0; s < size2; ++s) {
            for (int ch = 0; ch < channels; ++ch) {
                buffer.setSample(ch, start2 + s, src[s * numChannels + ch]);
            }
        }
    }

    fifo.finishedWrite(size1 + size2);
}

void AudioRingBuffer::readInterleavedSamples(float* interleavedData, int numChannels, int numSamples) {
    int start1, size1, start2, size2;
    fifo.prepareToRead(numSamples, start1, size1, start2, size2);

    int channels = juce::jmin(numChannels, channelCount);
    int totalRead = size1 + size2;

    // Interleave and read
    if (size1 > 0) {
        for (int s = 0; s < size1; ++s) {
            for (int ch = 0; ch < channels; ++ch) {
                interleavedData[s * numChannels + ch] = buffer.getSample(ch, start1 + s);
            }
        }
    }

    if (size2 > 0) {
        float* dst = interleavedData + size1 * numChannels;
        for (int s = 0; s < size2; ++s) {
            for (int ch = 0; ch < channels; ++ch) {
                dst[s * numChannels + ch] = buffer.getSample(ch, start2 + s);
            }
        }
    }

    // Zero-fill remainder
    if (totalRead < numSamples) {
        std::memset(interleavedData + totalRead * numChannels, 0,
                    (numSamples - totalRead) * numChannels * sizeof(float));
    }

    fifo.finishedRead(totalRead);
}
