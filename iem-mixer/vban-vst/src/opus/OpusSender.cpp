#include "OpusSender.h"

OpusSender::OpusSender()
    : Thread("Opus Sender")
{
    fifoBuffer.resize(FIFO_CAPACITY, 0);
}

OpusSender::~OpusSender() {
    stop();
}

void OpusSender::configure(const juce::String& destIP_, uint16_t port_) {
    destinationIP = destIP_;
    port = port_;
}

void OpusSender::start() {
    if (!running.exchange(true)) {
        fifo.reset();
        packetsSent = 0;
        sequenceNumber = 0;
        sending = false;
        startThread();
    }
}

void OpusSender::stop() {
    running = false;
    sending = false;
    stopThread(1000);
}

void OpusSender::pushFrame(const uint8_t* data, int size) {
    if (!running.load() || size <= 0 || size > 2000) return;

    // Each FIFO entry: [2 bytes size LE] [size bytes data]
    int totalSize = 2 + size;
    int freeSpace = fifo.getFreeSpace();
    if (freeSpace < totalSize) {
        // Queue full — drop this frame (acceptable for real-time audio)
        return;
    }

    // Write to FIFO
    int start1, size1, start2, size2;
    fifo.prepareToWrite(totalSize, start1, size1, start2, size2);

    // Build the entry: size prefix + data
    uint8_t sizeBytes[2] = {
        static_cast<uint8_t>(size & 0xFF),
        static_cast<uint8_t>((size >> 8) & 0xFF)
    };

    // Copy into fifo buffer, handling wrap-around
    int written = 0;
    auto copyByte = [&](uint8_t b) {
        if (written < size1) {
            fifoBuffer[start1 + written] = b;
        } else {
            fifoBuffer[start2 + (written - size1)] = b;
        }
        written++;
    };

    copyByte(sizeBytes[0]);
    copyByte(sizeBytes[1]);
    for (int i = 0; i < size; ++i) {
        copyByte(data[i]);
    }

    fifo.finishedWrite(totalSize);
}

void OpusSender::run() {
    juce::DatagramSocket sock;
    if (!sock.bindToPort(0)) {
        DBG("OpusSender: Failed to bind socket");
        running = false;
        return;
    }

    std::vector<uint8_t> sendBuf;
    sendBuf.resize(2048);

    while (!threadShouldExit() && running.load()) {
        int readyToRead = fifo.getNumReady();
        if (readyToRead < 2) {
            Thread::sleep(1);
            continue;
        }

        // Peek at frame size (2 bytes)
        int start1, size1, start2, size2;
        fifo.prepareToRead(2, start1, size1, start2, size2);

        uint8_t sizeBytes[2];
        if (size1 >= 2) {
            sizeBytes[0] = fifoBuffer[start1];
            sizeBytes[1] = fifoBuffer[start1 + 1];
        } else if (size1 == 1) {
            sizeBytes[0] = fifoBuffer[start1];
            sizeBytes[1] = fifoBuffer[start2];
        } else {
            sizeBytes[0] = fifoBuffer[start2];
            sizeBytes[1] = fifoBuffer[start2 + 1];
        }
        int frameSize = sizeBytes[0] | (sizeBytes[1] << 8);
        fifo.finishedRead(2);

        if (frameSize <= 0 || frameSize > 2000) {
            // Corrupt entry — skip
            continue;
        }

        // Check if full frame is available
        if (fifo.getNumReady() < frameSize) {
            Thread::sleep(1);
            continue;
        }

        // Read frame data
        fifo.prepareToRead(frameSize, start1, size1, start2, size2);

        // Build OIEM packet: [4 magic] [2 seq] [2 payload_size] [payload]
        uint16_t seq = sequenceNumber.fetch_add(1);
        int packetSize = OIEM_HEADER_SIZE + frameSize;

        // Header
        sendBuf[0] = OIEM_MAGIC[0]; // 'O'
        sendBuf[1] = OIEM_MAGIC[1]; // 'I'
        sendBuf[2] = OIEM_MAGIC[2]; // 'E'
        sendBuf[3] = OIEM_MAGIC[3]; // 'M'
        sendBuf[4] = static_cast<uint8_t>(seq & 0xFF);
        sendBuf[5] = static_cast<uint8_t>((seq >> 8) & 0xFF);
        sendBuf[6] = static_cast<uint8_t>(frameSize & 0xFF);
        sendBuf[7] = static_cast<uint8_t>((frameSize >> 8) & 0xFF);

        // Copy payload from FIFO, handling wrap-around
        int destOffset = OIEM_HEADER_SIZE;
        if (size1 > 0) {
            std::memcpy(&sendBuf[destOffset], &fifoBuffer[start1], size1);
            destOffset += size1;
        }
        if (size2 > 0) {
            std::memcpy(&sendBuf[destOffset], &fifoBuffer[start2], size2);
        }
        fifo.finishedRead(frameSize);

        // Send UDP
        int sent = sock.write(destinationIP, port, sendBuf.data(), packetSize);
        if (sent > 0) {
            packetsSent++;
            sending = true;
        } else {
            sending = false;
        }
    }
}
