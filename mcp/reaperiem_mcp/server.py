"""FastMCP server for REAPER IEM mixing control."""

from pathlib import Path

from fastmcp import FastMCP
from typing import Any

from .lib.config import load_config, Config
from .lib.reaper_http import ReaperHTTPClient
from .lib.ssh_client import SSHGitClient
from .tools import tracks
from .tools import mix
from .tools import git
from .tools import band
from .tools import routing

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


_ssh_client: SSHGitClient | None = None


def get_ssh_client() -> SSHGitClient:
    """Get or create SSH client for git operations."""
    global _ssh_client
    if _ssh_client is None:
        config = get_config()
        _ssh_client = SSHGitClient(
            host=config.ssh_host,
            username=config.ssh_username,
            repo_path=config.ssh_repo_path,
            key_path=config.ssh_key_path,
            password=config.ssh_password,
            port=config.ssh_port,
        )
    return _ssh_client


def get_config_dir() -> Path:
    """Get config directory path."""
    # Look relative to this file: server.py -> reaperiem_mcp -> mcp -> reaperiem -> config
    return Path(__file__).parent.parent.parent / "config"


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


@mcp.tool
def git_status() -> str:
    """Show git status of REAPER project on iem.lan.

    Shows which files have changed (e.g., after saving a project in REAPER).
    """
    client = get_ssh_client()
    return git.git_status(client)


@mcp.tool
def git_commit(message: str) -> str:
    """Commit REAPER project changes on iem.lan.

    Stages all changes and creates a commit with the given message.

    Args:
        message: Commit message describing the changes
    """
    client = get_ssh_client()
    return git.git_commit(client, message)


@mcp.tool
def git_push() -> str:
    """Push commits from iem.lan to GitHub.

    Pushes all local commits to the remote repository.
    """
    client = get_ssh_client()
    return git.git_push(client)


@mcp.tool
def git_log(count: int = 5) -> str:
    """Show recent commit history on iem.lan.

    Args:
        count: Number of commits to show (default 5)
    """
    client = get_ssh_client()
    return git.git_log(client, count)


@mcp.tool
def list_band_members() -> list[dict[str, Any]]:
    """List all band members and their output assignments.

    Shows each member's name, output track name, and Dante output channels.
    """
    return band.load_band_members(get_config_dir())


@mcp.tool
def list_input_tracks() -> list[dict[str, Any]]:
    """List all input track configurations.

    Shows each input's name, Dante input channel, and default level.
    """
    return band.load_input_tracks(get_config_dir())


@mcp.tool
def add_band_member(
    name: str, dante_output_l: int, dante_output_r: int
) -> dict[str, Any]:
    """Add a new band member to the configuration.

    Creates an output track assignment with stereo Dante outputs
    for the member's in-ear mix.

    Args:
        name: Member's name (will be uppercased for track name)
        dante_output_l: Left channel Dante output number
        dante_output_r: Right channel Dante output number
    """
    return band.add_band_member(
        get_config_dir(), name, dante_output_l, dante_output_r
    )


@mcp.tool
def add_input_track(
    name: str, dante_input: int, default_level_db: float = 0.0
) -> dict[str, Any]:
    """Add a new input track to the configuration.

    Args:
        name: Track name (convention: "NAME type", e.g., "MAREK mic")
        dante_input: Dante input channel number
        default_level_db: Default send level for this input (0.0 = unity)
    """
    return band.add_input_track(
        get_config_dir(), name, dante_input, default_level_db
    )


@mcp.tool
async def set_hardware_output(
    track_index: int, channel_l: int, channel_r: int
) -> str:
    """Set hardware output routing for a track (LIVE, no restart needed).

    Routes a track's output to specific Dante hardware output channels.
    This runs a ReaScript inside REAPER to configure the routing.

    Args:
        track_index: Track number (1-based)
        channel_l: Left Dante output channel (1-based, e.g., 25)
        channel_r: Right Dante output channel (1-based, e.g., 26)

    Returns:
        Result message confirming the routing change
    """
    config = get_config()
    if not config.action_set_hardware_output:
        return "Error: action_set_hardware_output not configured in reaper_config.yaml"

    client = get_reaper_client()
    return await routing.set_hardware_output(
        client, track_index, channel_l, channel_r, config.action_set_hardware_output
    )


if __name__ == "__main__":
    mcp.run()
