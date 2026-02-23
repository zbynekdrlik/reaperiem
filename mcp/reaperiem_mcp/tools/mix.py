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
