#include "OpusDecoderWrapper.h"

OpusDecoderWrapper::OpusDecoderWrapper() {
}

OpusDecoderWrapper::~OpusDecoderWrapper() {
    if (decoder) {
        opus_decoder_destroy(decoder);
        decoder = nullptr;
    }
}

bool OpusDecoderWrapper::configure(int sampleRate, int channels) {
    if (decoder) {
        opus_decoder_destroy(decoder);
        decoder = nullptr;
    }

    numChannels = channels;

    int error = 0;
    decoder = opus_decoder_create(sampleRate, channels, &error);
    if (error != OPUS_OK || !decoder) {
        DBG("OpusDecoderWrapper: Failed to create decoder: " + juce::String(opus_strerror(error)));
        decoder = nullptr;
        return false;
    }

    return true;
}

int OpusDecoderWrapper::decode(const uint8_t* opusData, int opusSize,
                                float* outPCM, int maxSamplesPerChannel) {
    if (!decoder) return 0;

    int samplesDecoded = opus_decode_float(decoder, opusData, opusSize,
                                            outPCM, maxSamplesPerChannel, 0);
    if (samplesDecoded < 0) {
        DBG("OpusDecoderWrapper: Decode error: " + juce::String(opus_strerror(samplesDecoded)));
        return 0;
    }

    return samplesDecoded;
}
