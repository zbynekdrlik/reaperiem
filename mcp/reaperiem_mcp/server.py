"""FastMCP server for REAPER IEM mixing control."""

from fastmcp import FastMCP
from typing import Any

from .lib.config import load_config, Config
from .lib.reaper_http import ReaperHTTPClient
from .tools import tracks
from .tools import mix

mcp = FastMCP(
    "REAPER IEM Mixer",
    instructions="Control REAPER for in-ear monitor mixing. Use track and send controls to adjust personal mixes.",
)

# Global client (initialized on first use)
_reaper_client: ReaperHTTPClient | None = None
_config: Config | None = None


def get_config() -> Config:
    """Get or load configuration."""
    global _config
    if _config is None:
        _config = load_config()
    return _config


def get_reaper_client() -> ReaperHTTPClient:
    """Get or create REAPER HTTP client."""
    global _reaper_client
    if _reaper_client is None:
        config = get_config()
        _reaper_client = ReaperHTTPClient(
            host=config.reaper_host,
            port=config.reaper_port,
            username=config.reaper_username,
            password=config.reaper_password,
        )
    return _reaper_client


@mcp.tool
def ping() -> str:
    """Test connectivity to MCP server."""
    return "pong - REAPER IEM Mixer is running"


@mcp.tool
async def list_tracks() -> list[dict[str, Any]]:
    """List all tracks in current REAPER project.

    Returns track index, name, volume, pan, and flags.
    """
    client = get_reaper_client()
    return await tracks.list_tracks(client)


@mcp.tool
async def get_track(index: int) -> dict[str, Any]:
    """Get detailed info for a specific track.

    Args:
        index: Track number (1-based)
    """
    client = get_reaper_client()
    return await tracks.get_track_info(client, index)


@mcp.tool
async def set_track_volume(index: int, volume_db: float) -> str:
    """Set track volume.

    Args:
        index: Track number (1-based)
        volume_db: Volume in dB (0.0 = unity, -inf to +12 typical range)
    """
    client = get_reaper_client()
    return await tracks.set_track_volume(client, index, volume_db)


@mcp.tool
async def mute_track(index: int, mute: bool = True) -> str:
    """Mute or unmute a track.

    Args:
        index: Track number (1-based)
        mute: True to mute, False to unmute
    """
    client = get_reaper_client()
    return await tracks.mute_track(client, index, mute)


@mcp.tool
async def solo_track(index: int, solo: bool = True) -> str:
    """Solo or unsolo a track.

    Args:
        index: Track number (1-based)
        solo: True to solo, False to unsolo
    """
    client = get_reaper_client()
    return await tracks.solo_track(client, index, solo)


@mcp.tool
async def set_send_level(
    track_index: int, send_index: int, level_db: float
) -> str:
    """Set send level from an input track to an output bus.

    This controls how much of a specific input (e.g., "MAREK mic")
    goes to a specific band member's IEM mix.

    Args:
        track_index: Source track number (1-based)
        send_index: Send/output bus number (1-based)
        level_db: Level in dB (0.0 = unity, -inf to +12 range)
    """
    client = get_reaper_client()
    return await mix.set_send_level(client, track_index, send_index, level_db)


@mcp.tool
async def adjust_send_level(
    track_index: int, send_index: int, adjustment_db: float
) -> str:
    """Adjust send level relatively.

    Args:
        track_index: Source track number (1-based)
        send_index: Send/output bus number (1-based)
        adjustment_db: Relative adjustment (+3.0 = boost 3dB, -6.0 = cut 6dB)
    """
    client = get_reaper_client()
    return await mix.adjust_send_level(
        client, track_index, send_index, adjustment_db
    )


if __name__ == "__main__":
    mcp.run()
