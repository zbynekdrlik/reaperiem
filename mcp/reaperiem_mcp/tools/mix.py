"""MCP tools for send/mix control - the core of IEM mixing."""

import math
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

    # Convert from user range (-1.0 to 1.0) to REAPER range (0.0 to 1.0)
    reaper_pan = (pan + 1.0) / 2.0

    await client.set_send_pan(track_index, send_index, reaper_pan)
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
    REAPER's TRACK response includes meter values at indices 12-13
    of the tab-separated TRACK array (peak_l, peak_r as linear 0.0-1.0).

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

    # REAPER TRACK response array indices:
    # [0]=index, [1]=name, [2]=flags, [3]=vol, [4]=pan, ...
    # [12]=peak_l (linear), [13]=peak_r (linear)
    # Meter values are linear 0.0-1.0; convert to dB

    # Extract peak values from the response
    peak_l_linear = 0.0
    peak_r_linear = 0.0

    if isinstance(track_data, list) and len(track_data) > 12:
        peak_l_linear = float(track_data[12])
        if len(track_data) > 13:
            peak_r_linear = float(track_data[13])
        else:
            # Mono track: use same value for both channels
            peak_r_linear = peak_l_linear

    peak_l_db = linear_to_db(peak_l_linear)
    peak_r_db = linear_to_db(peak_r_linear)

    # RMS approximation from peak: RMS ~= peak * 0.707 (-3dB) for typical audio
    # This is a reasonable approximation when REAPER doesn't expose separate RMS
    rms_factor = 1.0 / math.sqrt(2)  # 0.7071...
    rms_l_db = linear_to_db(peak_l_linear * rms_factor)
    rms_r_db = linear_to_db(peak_r_linear * rms_factor)

    return {
        "track_index": track_index,
        "peak_l": peak_l_db,
        "peak_r": peak_r_db,
        "rms_l": rms_l_db,
        "rms_r": rms_r_db,
    }


def db_to_linear(db: float) -> float:
    """Convert dB to linear gain."""
    return 10 ** (db / 20)


def linear_to_db(linear: float) -> float:
    """Convert linear gain to dB."""
    if linear <= 0:
        return float("-inf")
    return 20 * math.log10(linear)
