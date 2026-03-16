#!/usr/bin/env python3
"""
Audio Pipeline E2E Test

Tests the full audio streaming pipeline:
  ReaStream UDP -> iem-mixer-app -> Opus encode -> WebSocket -> this script

Sends synthetic ReaStream UDP packets to localhost:4711 and verifies that
Opus-encoded audio frames arrive on the /ws/audio WebSocket endpoint.

Usage:
  python test_audio_pipeline.py [--host HOST] [--port PORT] [--timeout SECS]
  python test_audio_pipeline.py --udp-only          # Just send UDP packets
  python test_audio_pipeline.py --diagnose           # Check UDP port and firewall
"""

import argparse
import asyncio
import json
import math
import socket
import struct
import sys
import threading
import time

# ReaStream packet constants
REASTREAM_MAGIC = b"MRSR"
# 32-byte null-padded identifier
REASTREAM_ID = b"engineer" + b"\x00" * 24


def build_reastream_packet(channels=2, sample_rate=96000, samples_per_ch=512,
                           sample_offset=0):
    """Build a single ReaStream UDP packet with a 440Hz sine tone."""
    pkt = bytearray()
    pkt.extend(REASTREAM_MAGIC)

    num_samples = samples_per_ch * channels
    audio_bytes = num_samples * 4
    packet_size = 32 + 1 + 4 + 2 + audio_bytes
    pkt.extend(struct.pack("<I", packet_size))
    pkt.extend(REASTREAM_ID)
    pkt.append(channels)
    pkt.extend(struct.pack("<I", sample_rate))
    pkt.extend(struct.pack("<H", samples_per_ch))

    for i in range(samples_per_ch):
        t = (sample_offset + i) / sample_rate
        val = 0.3 * math.sin(2 * math.pi * 440 * t)
        for _ in range(channels):
            pkt.extend(struct.pack("<f", val))

    return bytes(pkt)


def send_reastream_udp(host="127.0.0.1", port=4711, duration=5,
                       sample_rate=96000, stop_event=None):
    """Send synthetic ReaStream packets for a given duration."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    samples_per_pkt = 512
    interval = samples_per_pkt / sample_rate
    offset = 0
    sent = 0
    start = time.time()

    while time.time() - start < duration:
        if stop_event and stop_event.is_set():
            break
        pkt = build_reastream_packet(
            channels=2,
            sample_rate=sample_rate,
            samples_per_ch=samples_per_pkt,
            sample_offset=offset,
        )
        sock.sendto(pkt, (host, port))
        offset += samples_per_pkt
        sent += 1
        time.sleep(interval)

    sock.close()
    return sent


def diagnose_udp(port=4711):
    """Check if UDP port 4711 is in use and can receive packets."""
    print(f"=== UDP Port {port} Diagnostics ===")

    # Check if port is already bound
    test_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        test_sock.bind(("127.0.0.1", port))
        print(f"  Port {port}: AVAILABLE (not in use)")
        print("  WARNING: iem-mixer-app may not be listening!")
        test_sock.close()
        return False
    except OSError:
        print(f"  Port {port}: IN USE (something is listening)")
        test_sock.close()

    # Try sending a packet
    sender = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    pkt = build_reastream_packet(samples_per_ch=64)
    try:
        sender.sendto(pkt, ("127.0.0.1", port))
        print(f"  Sent test packet to 127.0.0.1:{port}: OK")
    except Exception as e:
        print(f"  Sent test packet to 127.0.0.1:{port}: FAILED ({e})")

    try:
        sender.sendto(pkt, ("255.255.255.255", port))
        print(f"  Sent test packet to 255.255.255.255:{port}: OK")
    except Exception as e:
        print(f"  Sent test packet to 255.255.255.255:{port}: FAILED ({e})")

    sender.close()
    return True


async def test_audio_ws(host="localhost", port=80, timeout=15, pin="1177"):
    """Connect to audio WebSocket and verify Opus frames arrive."""
    try:
        import websockets
    except ImportError:
        print("ERROR: websockets library not installed")
        print("Install with: pip install websockets")
        return None

    import urllib.request

    # Step 1: Authenticate as engineer
    auth_url = f"http://{host}:{port}/api/auth"
    auth_body = json.dumps({"member": "engineer", "pin": pin}).encode()
    req = urllib.request.Request(
        auth_url,
        data=auth_body,
        headers={"Content-Type": "application/json"},
    )

    try:
        resp = urllib.request.urlopen(req, timeout=5)
        token_data = json.loads(resp.read())
        token = token_data["token"]
        print(f"Authenticated as engineer (token expires in {token_data['expires_in']}s)")
    except Exception as e:
        print(f"FAIL: Cannot authenticate: {e}")
        return False

    # Step 2: Connect to audio WebSocket
    ws_url = f"ws://{host}:{port}/ws/audio?token={token}"
    stop_event = threading.Event()

    try:
        async with websockets.connect(ws_url, open_timeout=5) as ws:
            # Send ListenStart
            await ws.send(json.dumps({"cmd": "ListenStart"}))
            print("Sent ListenStart")

            # Start UDP sender in background
            udp_thread = threading.Thread(
                target=send_reastream_udp,
                kwargs={
                    "host": "127.0.0.1",
                    "port": 4711,
                    "duration": timeout,
                    "stop_event": stop_event,
                },
            )
            udp_thread.start()

            # Wait for audio frames
            binary_frames = 0
            statuses = []
            deadline = time.time() + timeout

            while time.time() < deadline:
                try:
                    msg = await asyncio.wait_for(ws.recv(), timeout=2.0)
                    if isinstance(msg, bytes):
                        binary_frames += 1
                        if binary_frames == 1:
                            print(f"  First Opus frame: {len(msg)} bytes")
                        if binary_frames >= 5:
                            break
                    else:
                        data = json.loads(msg)
                        status = data.get("data", {}).get("status", "unknown")
                        statuses.append(status)
                        print(f"  Status: {status}")
                        if status == "no_source":
                            # Wait a bit more - UDP sender may not have started yet
                            pass
                except asyncio.TimeoutError:
                    continue

            stop_event.set()
            udp_thread.join(timeout=2)

            if binary_frames >= 5:
                print(f"PASS: Received {binary_frames} Opus frames")
                return True
            elif binary_frames > 0:
                print(f"PARTIAL: Received {binary_frames} Opus frames (expected >= 5)")
                return True
            else:
                print(f"FAIL: No Opus frames received. Statuses: {statuses}")
                return False

    except Exception as e:
        stop_event.set()
        print(f"FAIL: WebSocket error: {e}")
        return False


def main():
    parser = argparse.ArgumentParser(description="Audio pipeline E2E test")
    parser.add_argument("--host", default="localhost")
    parser.add_argument("--port", type=int, default=80)
    parser.add_argument("--timeout", type=int, default=15)
    parser.add_argument("--pin", default="1177", help="Engineer PIN")
    parser.add_argument("--udp-only", action="store_true",
                        help="Only send UDP packets (no WebSocket test)")
    parser.add_argument("--diagnose", action="store_true",
                        help="Run UDP diagnostics only")
    args = parser.parse_args()

    if args.diagnose:
        diagnose_udp()
        return 0

    if args.udp_only:
        print(f"Sending ReaStream UDP to 127.0.0.1:4711 for {args.timeout}s...")
        count = send_reastream_udp(duration=args.timeout)
        print(f"Sent {count} packets")
        return 0

    print(f"Testing audio pipeline: UDP -> App ({args.host}:{args.port}) -> WebSocket")
    print(f"Timeout: {args.timeout}s")
    print()

    result = asyncio.run(test_audio_ws(args.host, args.port, args.timeout, args.pin))

    if result is True:
        print("\nPASS: Audio pipeline is working")
        return 0
    elif result is None:
        print("\nSKIP: Missing dependencies")
        return 0
    else:
        print("\nFAIL: Audio pipeline broken")
        return 1


if __name__ == "__main__":
    sys.exit(main())
