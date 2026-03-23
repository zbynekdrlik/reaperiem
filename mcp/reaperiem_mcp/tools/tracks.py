"""MCP tools for REAPER track control."""

import asyncio
from typing import Any

from ..lib.reaper_http import ReaperHTTPClient


async def list_tracks(client: ReaperHTTPClient) -> list[dict[str, Any]]:
    """List all tracks in current REAPER project."""
    count = await client.get_track_count()
    tracks = []
    for i in range(1, count + 1):
        track_data = await client.get_track(i)
        # Parse TRACK response: [index, name, flags, vol, pan, ...]
        # Flags: bit 3 (0x08) = muted, bits 4-5 (0x30) = solo
        track_info = track_data.get("TRACK", [])
        flags = int(track_info[2]) if len(track_info) > 2 else 0
        tracks.append({
            "index": i,
            "name": track_info[1] if len(track_info) > 1 else f"Track {i}",
            "volume": float(track_info[3]) if len(track_info) > 3 else 1.0,
            "pan": float(track_info[4]) if len(track_info) > 4 else 0.0,
            "muted": bool(flags & 0x08),
            "soloed": bool(flags & 0x30),
            "send_count": int(track_info[9]) if len(track_info) > 9 else 0,
            "recv_count": int(track_info[10]) if len(track_info) > 10 else 0,
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


async def rename_track(
    client: ReaperHTTPClient,
    track_index: int,
    new_name: str,
    action_id: str,
) -> str:
    """Rename a REAPER track via EXTSTATE + action trigger.

    Args:
        client: REAPER HTTP client
        track_index: Track number (1-based)
        new_name: New track name
        action_id: ReaScript action ID (e.g., "_RSxxxx...")

    Returns:
        Result message from the script
    """
    section = "reaperiem"

    # Set parameters via ExtState
    await client.set_extstate(section, "rename_track_index", str(track_index))
    await client.set_extstate(section, "rename_track_name", new_name)

    # Small delay to ensure ExtState is set
    await asyncio.sleep(0.05)

    # Trigger the ReaScript
    await client.trigger_action(action_id)

    # Wait for script to complete and check result
    await asyncio.sleep(0.1)
    result = await client.get_extstate(section, "rename_result")

    return result or "Action triggered (no result returned)"


async def list_track_fx(
    client: ReaperHTTPClient,
    action_id: str,
) -> str:
    """List FX chains on all mic/guitar tracks via check_input_trim.lua.

    Returns current trim state and FX info for all mic/gtr tracks.
    """
    section = "reaperiem"

    # Trigger the check script
    await client.trigger_action(action_id)

    # Wait for script to complete
    await asyncio.sleep(0.3)
    result = await client.get_extstate(section, "trim_check")

    return result or "No result returned from check_input_trim.lua"
