"""MCP tools for REAPER track control."""

from typing import Any

from ..lib.reaper_http import ReaperHTTPClient


async def list_tracks(client: ReaperHTTPClient) -> list[dict[str, Any]]:
    """List all tracks in current REAPER project."""
    count = await client.get_track_count()
    tracks = []
    for i in range(1, count + 1):
        track_data = await client.get_track(i)
        tracks.append({
            "index": i,
            "data": track_data,
        })
    return tracks


async def get_track_info(client: ReaperHTTPClient, index: int) -> dict[str, Any]:
    """Get detailed info for a specific track."""
    return await client.get_track(index)


async def set_track_volume(
    client: ReaperHTTPClient, index: int, volume_db: float
) -> str:
    """Set track volume in dB. 0.0 = unity gain."""
    # Convert dB to linear (REAPER uses 1.0 = 0dB)
    linear = 10 ** (volume_db / 20)
    await client.set_track_volume(index, linear)
    return f"Track {index} volume set to {volume_db}dB"


async def mute_track(
    client: ReaperHTTPClient, index: int, mute: bool = True
) -> str:
    """Mute or unmute a track."""
    await client.set_track_mute(index, mute)
    state = "muted" if mute else "unmuted"
    return f"Track {index} {state}"


async def solo_track(
    client: ReaperHTTPClient, index: int, solo: bool = True
) -> str:
    """Solo or unsolo a track."""
    await client.set_track_solo(index, solo)
    state = "soloed" if solo else "unsoloed"
    return f"Track {index} {state}"
