"""MCP tools for band member management."""

from pathlib import Path
from typing import Any

import yaml


def load_band_members(config_dir: Path | str) -> list[dict[str, Any]]:
    """Load band members from YAML config."""
    path = Path(config_dir) / "band_members.yaml"
    with path.open() as f:
        data = yaml.safe_load(f)
    return data.get("band_members", [])


def load_input_tracks(config_dir: Path | str) -> list[dict[str, Any]]:
    """Load input track routing from YAML config."""
    path = Path(config_dir) / "input_tracks.yaml"
    with path.open() as f:
        data = yaml.safe_load(f)
    return data.get("input_tracks", [])


def save_band_members(
    config_dir: Path | str, members: list[dict[str, Any]]
) -> None:
    """Save band members to YAML config."""
    path = Path(config_dir) / "band_members.yaml"
    with path.open("w") as f:
        yaml.safe_dump({"band_members": members}, f, default_flow_style=False)


def save_input_tracks(
    config_dir: Path | str, tracks: list[dict[str, Any]]
) -> None:
    """Save input tracks to YAML config."""
    path = Path(config_dir) / "input_tracks.yaml"
    with path.open("w") as f:
        yaml.safe_dump({"input_tracks": tracks}, f, default_flow_style=False)


def add_band_member(
    config_dir: Path | str,
    name: str,
    dante_output_l: int,
    dante_output_r: int,
) -> dict[str, Any]:
    """Add a new band member to config."""
    members = load_band_members(config_dir)

    # Generate next ID
    next_id = max((m["id"] for m in members), default=0) + 1

    # Create track name following convention: "NAME inear"
    output_track_name = f"{name.upper()} inear"

    new_member = {
        "id": next_id,
        "name": name,
        "output_track_name": output_track_name,
        "dante_output_L": dante_output_l,
        "dante_output_R": dante_output_r,
    }

    members.append(new_member)
    save_band_members(config_dir, members)

    return new_member


def add_input_track(
    config_dir: Path | str,
    name: str,
    dante_input: int,
    default_level_db: float = 0.0,
) -> dict[str, Any]:
    """Add a new input track to config."""
    tracks = load_input_tracks(config_dir)

    new_track = {
        "name": name,
        "dante_input": dante_input,
        "default_level_db": default_level_db,
    }

    tracks.append(new_track)
    save_input_tracks(config_dir, tracks)

    return new_track
