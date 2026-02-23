"""MCP tools for send/mix control - the core of IEM mixing."""

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


def db_to_linear(db: float) -> float:
    """Convert dB to linear gain."""
    return 10 ** (db / 20)


def linear_to_db(linear: float) -> float:
    """Convert linear gain to dB."""
    import math
    if linear <= 0:
        return float("-inf")
    return 20 * math.log10(linear)
