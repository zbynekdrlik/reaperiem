"""Tools for track routing and hardware outputs."""

import asyncio
from ..lib.reaper_http import ReaperHTTPClient


async def set_hardware_output(
    client: ReaperHTTPClient,
    track_index: int,
    channel_l: int,
    channel_r: int,
    action_id: str,
) -> str:
    """Set hardware output for a track.

    This sets ExtState parameters and triggers a ReaScript
    that configures the hardware output routing.

    Args:
        client: REAPER HTTP client
        track_index: Track number (1-based)
        channel_l: Left output channel (1-based Dante channel)
        channel_r: Right output channel (1-based Dante channel)
        action_id: ReaScript action ID (e.g., "_RSxxxx...")

    Returns:
        Result message from the script
    """
    section = "reaperiem"

    # Set parameters via ExtState
    await client.set_extstate(section, "hw_track_index", str(track_index))
    await client.set_extstate(section, "hw_channel_l", str(channel_l))
    await client.set_extstate(section, "hw_channel_r", str(channel_r))

    # Small delay to ensure ExtState is set
    await asyncio.sleep(0.05)

    # Trigger the ReaScript
    await client.trigger_action(action_id)

    # Wait for script to complete and check result
    await asyncio.sleep(0.1)
    result = await client.get_extstate(section, "hw_result")

    return result or "Action triggered (no result returned)"
