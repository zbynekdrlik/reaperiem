#pragma once
#include <cstdint>
#include <array>
#include <cstring>

namespace vban {

#pragma pack(push, 1)
struct Header {
    uint32_t magic;          // "VBAN" = 0x4E414256 (little-endian)
    uint8_t  format_SR;      // Sample rate index (bits 0-4) | Protocol (bits 5-7)
    uint8_t  format_nbs;     // Samples per frame - 1 (0-255 = 1-256 samples)
    uint8_t  format_nbc;     // Channels - 1 (0-255 = 1-256 channels)
    uint8_t  format_bit;     // Bit format (bits 0-2) | Reserved (bit 3) | Codec (bits 4-7)
    char     streamname[16]; // Stream name (null-padded)
    uint32_t nuFrame;        // Frame counter (incrementing)
};
#pragma pack(pop)

static_assert(sizeof(Header) == 28, "VBAN header must be 28 bytes");

// Magic value "VBAN" in little-endian
constexpr uint32_t MAGIC = 0x4E414256;

// Default port
constexpr uint16_t DEFAULT_PORT = 7980;

// Packet sizes
constexpr size_t MAX_PACKET_SIZE = 1464;
constexpr size_t HEADER_SIZE = sizeof(Header);
constexpr size_t MAX_DATA_SIZE = MAX_PACKET_SIZE - HEADER_SIZE;
constexpr size_t STREAM_NAME_SIZE = 16;

// Sample rates table (index 0-20)
constexpr std::array<int, 21> SAMPLE_RATES = {
    6000,   12000,  24000,  48000,  96000,  192000, 384000,   // 0-6
    8000,   16000,  32000,  64000,  128000, 256000, 512000,   // 7-13
    11025,  22050,  44100,  88200,  176400, 352800, 705600    // 14-20
};

// Masks
constexpr uint8_t SR_MASK = 0x1F;
constexpr uint8_t PROTOCOL_MASK = 0xE0;
constexpr uint8_t DATATYPE_MASK = 0x07;
constexpr uint8_t CODEC_MASK = 0xF0;

// Protocol types
constexpr uint8_t PROTOCOL_AUDIO = 0x00;
constexpr uint8_t PROTOCOL_SERIAL = 0x20;
constexpr uint8_t PROTOCOL_TEXT = 0x40;

// Data types (bit format)
enum class DataType : uint8_t {
    BYTE8   = 0,
    INT16   = 1,
    INT24   = 2,
    INT32   = 3,
    FLOAT32 = 4,
    FLOAT64 = 5
};

// Codec types
constexpr uint8_t CODEC_PCM = 0x00;

// Sample sizes in bytes
constexpr std::array<int, 6> SAMPLE_SIZES = { 1, 2, 3, 4, 4, 8 };

// Helper functions
inline int getSampleRateIndex(int sampleRate) {
    for (size_t i = 0; i < SAMPLE_RATES.size(); ++i) {
        if (SAMPLE_RATES[i] == sampleRate) {
            return static_cast<int>(i);
        }
    }
    // Default to 48000 (index 3) if not found
    return 3;
}

inline int getSampleRateFromIndex(int index) {
    if (index >= 0 && index < static_cast<int>(SAMPLE_RATES.size())) {
        return SAMPLE_RATES[index];
    }
    return 48000;
}

inline int getSampleSize(DataType type) {
    int index = static_cast<int>(type);
    if (index >= 0 && index < static_cast<int>(SAMPLE_SIZES.size())) {
        return SAMPLE_SIZES[index];
    }
    return 2; // Default to INT16
}

inline bool isValidHeader(const Header& header) {
    return header.magic == MAGIC;
}

inline void setStreamName(Header& header, const char* name) {
    std::memset(header.streamname, 0, STREAM_NAME_SIZE);
    if (name) {
        std::strncpy(header.streamname, name, STREAM_NAME_SIZE - 1);
    }
}

} // namespace vban
