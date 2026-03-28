#include "OpusEncoderWrapper.h"

OpusEncoderWrapper::OpusEncoderWrapper() {
    encodedBuffer.resize(4000); // Max Opus packet size
}

OpusEncoderWrapper::~OpusEncoderWrapper() {
    if (encoder) {
        opus_encoder_destroy(encoder);
        encoder = nullptr;
    }
}

bool OpusEncoderWrapper::configure(int sampleRate, int channels, int bitrate) {
    if (encoder) {
        opus_encoder_destroy(encoder);
        encoder = nullptr;
    }

    numChannels = channels;
    accumBuffer.clear();
    accumBuffer.reserve(OPUS_FRAME_SAMPLES * numChannels * 2);

    int error = 0;
    encoder = opus_encoder_create(sampleRate, channels, OPUS_APPLICATION_AUDIO, &error);
    if (error != OPUS_OK || !encoder) {
        DBG("OpusEncoderWrapper: Failed to create encoder: " + juce::String(opus_strerror(error)));
        encoder = nullptr;
        return false;
    }

    opus_encoder_ctl(encoder, OPUS_SET_BITRATE(bitrate));
    // VBR enabled for better quality at the same bitrate
    opus_encoder_ctl(encoder, OPUS_SET_VBR(1));
    // Signal type: music
    opus_encoder_ctl(encoder, OPUS_SET_SIGNAL(OPUS_SIGNAL_MUSIC));
    // Forward Error Correction: embed redundant data from previous frame
    // so decoder can recover lost packets without retransmission
    opus_encoder_ctl(encoder, OPUS_SET_INBAND_FEC(1));
    opus_encoder_ctl(encoder, OPUS_SET_PACKET_LOSS_PERC(10));

    return true;
}

void OpusEncoderWrapper::encode(const float* interleaved, int numSamplesPerChannel,
                                 std::vector<std::vector<uint8_t>>& outFrames) {
    if (!encoder) return;

    // Append incoming interleaved samples to accumulation buffer
    int totalSamples = numSamplesPerChannel * numChannels;
    accumBuffer.insert(accumBuffer.end(), interleaved, interleaved + totalSamples);

    // Encode complete 20ms frames
    int frameSizeInterleaved = OPUS_FRAME_SAMPLES * numChannels;
    while (static_cast<int>(accumBuffer.size()) >= frameSizeInterleaved) {
        int len = opus_encode_float(encoder, accumBuffer.data(),
                                     OPUS_FRAME_SAMPLES,
                                     encodedBuffer.data(),
                                     static_cast<int>(encodedBuffer.size()));
        if (len > 0) {
            outFrames.emplace_back(encodedBuffer.data(), encodedBuffer.data() + len);
        }

        // Remove consumed samples
        accumBuffer.erase(accumBuffer.begin(), accumBuffer.begin() + frameSizeInterleaved);
    }
}
