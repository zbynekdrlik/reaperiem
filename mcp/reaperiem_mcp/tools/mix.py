"""MCP tools for send/mix control - the core of IEM mixing."""

import math
import shlex
from typing import Any

from ..lib.reaper_http import ReaperHTTPClient


async def set_send_level(
    client: ReaperHTTPClient,
    track_index: int,
    send_index: int,
    level_db: float,
) -> str:
    """Set send level from track to output bus.

    This is the core function for IEM mixing - each band member's
    output bus receives sends from all input tracks.
    """
    # Convert dB to linear
    linear = 10 ** (level_db / 20)
    await client.set_send_volume(track_index, send_index, linear)
    return f"Send from track {track_index} to send {send_index} set to {level_db}dB"


async def set_send_pan(
    client: ReaperHTTPClient,
    track_index: int,
    send_index: int,
    pan: float,
) -> str:
    """Set send pan position for stereo IEM mix.

    Allows band members to position audio sources left/right
    in their stereo in-ear mix.

    Args:
        client: REAPER HTTP client
        track_index: Source track number (1-based)
        send_index: Send/output bus number
        pan: Pan position from -1.0 (left) to 1.0 (right), 0.0 = center
    """
    if pan < -1.0 or pan > 1.0:
        raise ValueError(f"Pan must be between -1.0 and 1.0, got {pan}")

    # REAPER SET PAN uses -1.0..1.0 natively (verified empirically 2026-03-01)
    await client.set_send_pan(track_index, send_index, pan)
    return f"Send from track {track_index} to send {send_index} pan set to {pan}"


async def set_send_mute(
    client: ReaperHTTPClient,
    track_index: int,
    send_index: int,
    mute: bool,
) -> str:
    """Mute or unmute a send from a track to an output bus.

    Allows band members to mute specific input sources in their
    in-ear mix without changing the send level.

    Args:
        client: REAPER HTTP client
        track_index: Source track number (1-based)
        send_index: Send/output bus number
        mute: True to mute, False to unmute
    """
    await client.set_send_mute(track_index, send_index, mute)
    state = "muted" if mute else "unmuted"
    return f"Send from track {track_index} to send {send_index} {state}"


async def adjust_send_level(
    client: ReaperHTTPClient,
    track_index: int,
    send_index: int,
    adjustment_db: float,
) -> str:
    """Adjust send level relatively by dB amount.

    Uses REAPER's relative adjustment feature (+ prefix).
    """
    # REAPER accepts +value or -value for relative adjustment
    sign = "+" if adjustment_db >= 0 else ""
    await client.send_command(
        f"SET/TRACK/{track_index}/SEND/{send_index}/VOL/{sign}{adjustment_db}"
    )
    return f"Send adjusted by {adjustment_db}dB"


async def get_track_meter(
    client: ReaperHTTPClient,
    track_index: int,
) -> dict[str, Any]:
    """Get real-time meter levels for a track.

    Returns peak and RMS levels in dB for both channels.
    REAPER TRACK response (after keyword stripping by _parse_response):
      [0]=index, [1]=name, [2]=flags, [3]=vol, [4]=pan,
      [5]=VU_PEAK_L (centibels), [6]=VU_PEAK_R (centibels), ...

    Meter values are integer centibels (e.g., -231 = -2.31 dB).
    Floor: -1500 cb (-15.0 dB) = silence (verified empirically 2026-02-28).

    Args:
        client: REAPER HTTP client
        track_index: Track number (1-based)

    Returns:
        Dict with peak_l, peak_r, rms_l, rms_r (all in dB), and track_index

    Raises:
        ValueError: If track does not exist
    """
    result = await client.send_command(f"TRACK/{track_index}")

    track_data = result.get("TRACK")
    if track_data is None:
        raise ValueError(f"Track {track_index} not found")

    # Extract peak values from centibels (indices 5,6 after keyword strip)
    peak_l_db = float("-inf")
    peak_r_db = float("-inf")

    if isinstance(track_data, list) and len(track_data) > 6:
        cb_l = float(track_data[5])
        cb_r = float(track_data[6])

        peak_l_db = cb_l / 100.0 if cb_l > -1500.0 else float("-inf")
        peak_r_db = cb_r / 100.0 if cb_r > -1500.0 else float("-inf")
    elif isinstance(track_data, list) and len(track_data) > 5:
        cb_l = float(track_data[5])
        peak_l_db = cb_l / 100.0 if cb_l > -1500.0 else float("-inf")
        peak_r_db = peak_l_db  # Mono track

    # RMS approximation: peak - 3 dB
    rms_l_db = peak_l_db - 3.0 if peak_l_db != float("-inf") else float("-inf")
    rms_r_db = peak_r_db - 3.0 if peak_r_db != float("-inf") else float("-inf")

    return {
        "track_index": track_index,
        "peak_l": peak_l_db,
        "peak_r": peak_r_db,
        "rms_l": rms_l_db,
        "rms_r": rms_r_db,
    }


def cb_to_linear(cb: float) -> float:
    """Convert centibels to linear gain.

    REAPER HTTP API meter floor: -1500 centibels = silence.
    """
    if cb <= -1500.0:
        return 0.0
    return 10.0 ** (cb / 100.0 / 20.0)


def db_to_linear(db: float) -> float:
    """Convert dB to linear gain."""
    return 10 ** (db / 20)


def linear_to_db(linear: float) -> float:
    """Convert linear gain to dB."""
    if linear <= 0:
        return float("-inf")
    return 20 * math.log10(linear)
