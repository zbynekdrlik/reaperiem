#include "OpusReceiver.h"

OpusReceiver::OpusReceiver()
    : Thread("Opus Receiver")
{
    fifoBuffer.resize(FIFO_CAPACITY, 0);
}

OpusReceiver::~OpusReceiver() {
    stop();
}

void OpusReceiver::configure(const juce::String& serverIP_, uint16_t serverPort_) {
    serverIP = serverIP_;
    serverPort = serverPort_;
}

void OpusReceiver::start() {
    if (!running.exchange(true)) {
        fifo.reset();
        packetsReceived = 0;
        receiving = false;
        boundPort = 0;
        startThread();
    }
}

void OpusReceiver::stop() {
    running = false;
    receiving = false;
    stopThread(1000);
}

int OpusReceiver::popFrame(uint8_t* outData, int maxSize) {
    int readyToRead = fifo.getNumReady();
    if (readyToRead < 2)
        return 0;

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
        // Corrupt entry — discard
        return 0;
    }

    // Check if full frame is available
    if (fifo.getNumReady() < frameSize) {
        return 0;
    }

    if (frameSize > maxSize) {
        // Frame too large for output buffer — discard it
        fifo.prepareToRead(frameSize, start1, size1, start2, size2);
        fifo.finishedRead(frameSize);
        return 0;
    }

    // Read frame data
    fifo.prepareToRead(frameSize, start1, size1, start2, size2);

    int destOffset = 0;
    if (size1 > 0) {
        std::memcpy(outData + destOffset, &fifoBuffer[start1], size1);
        destOffset += size1;
    }
    if (size2 > 0) {
        std::memcpy(outData + destOffset, &fifoBuffer[start2], size2);
    }
    fifo.finishedRead(frameSize);

    return frameSize;
}

void OpusReceiver::sendHeartbeat(juce::DatagramSocket& sock) {
    // Heartbeat: OIEM magic + 0x0000 seq + 0x0000 payload_size = 8 bytes
    uint8_t heartbeat[OIEM_HEADER_SIZE] = {
        OIEM_MAGIC[0], OIEM_MAGIC[1], OIEM_MAGIC[2], OIEM_MAGIC[3],
        0x00, 0x00,  // sequence = 0
        0x00, 0x00   // payload_size = 0
    };
    sock.write(serverIP, serverPort, heartbeat, OIEM_HEADER_SIZE);
}

void OpusReceiver::run() {
    juce::DatagramSocket sock;
    // Bind to ephemeral port (OS assigns)
    if (!sock.bindToPort(0)) {
        DBG("OpusReceiver: Failed to bind socket");
        running = false;
        return;
    }

    boundPort = static_cast<uint16_t>(sock.getBoundPort());
    DBG("OpusReceiver: Bound to port " + juce::String(boundPort.load()));

    // Send initial heartbeat immediately
    sendHeartbeat(sock);
    auto lastHeartbeat = juce::Time::getMillisecondCounter();

    std::vector<uint8_t> recvBuf(2048);

    while (!threadShouldExit() && running.load()) {
        // Send heartbeat every 2 seconds
        auto now = juce::Time::getMillisecondCounter();
        if (now - lastHeartbeat >= HEARTBEAT_INTERVAL_MS) {
            sendHeartbeat(sock);
            lastHeartbeat = now;
        }

        // Wait for data with short timeout so we can check heartbeat timing
        if (!sock.waitUntilReady(true, 100)) {
            continue;
        }

        juce::String senderIP;
        int senderPort = 0;
        int bytesRead = sock.read(recvBuf.data(), static_cast<int>(recvBuf.size()),
                                   false, senderIP, senderPort);

        if (bytesRead < OIEM_HEADER_SIZE)
            continue;

        // Validate OIEM magic
        if (recvBuf[0] != OIEM_MAGIC[0] || recvBuf[1] != OIEM_MAGIC[1] ||
            recvBuf[2] != OIEM_MAGIC[2] || recvBuf[3] != OIEM_MAGIC[3]) {
            continue;
        }

        // Parse header
        // uint16_t seq = recvBuf[4] | (recvBuf[5] << 8);  // available for future jitter stats
        uint16_t payloadSize = recvBuf[6] | (recvBuf[7] << 8);

        if (payloadSize == 0) {
            // Heartbeat response or empty packet — ignore
            continue;
        }

        if (payloadSize > bytesRead - OIEM_HEADER_SIZE) {
            // Truncated packet
            continue;
        }

        if (payloadSize > 2000) {
            // Unreasonably large
            continue;
        }

        // Push Opus frame to FIFO: [2 bytes size LE] [payload]
        int totalSize = 2 + payloadSize;
        int freeSpace = fifo.getFreeSpace();
        if (freeSpace < totalSize) {
            // Queue full — drop oldest by not writing (acceptable for real-time)
            continue;
        }

        int start1, size1, start2, size2;
        fifo.prepareToWrite(totalSize, start1, size1, start2, size2);

        // Build the entry: size prefix + data
        uint8_t sizeLE[2] = {
            static_cast<uint8_t>(payloadSize & 0xFF),
            static_cast<uint8_t>((payloadSize >> 8) & 0xFF)
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

        copyByte(sizeLE[0]);
        copyByte(sizeLE[1]);
        const uint8_t* payload = recvBuf.data() + OIEM_HEADER_SIZE;
        for (int i = 0; i < payloadSize; ++i) {
            copyByte(payload[i]);
        }

        fifo.finishedWrite(totalSize);

        packetsReceived++;
        receiving = true;
    }
}
