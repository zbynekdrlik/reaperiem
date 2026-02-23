"""FastMCP server for REAPER IEM mixing control."""

from fastmcp import FastMCP
from typing import Any

from .lib.config import load_config, Config
from .lib.reaper_http import ReaperHTTPClient
from .tools import tracks

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


if __name__ == "__main__":
    mcp.run()
