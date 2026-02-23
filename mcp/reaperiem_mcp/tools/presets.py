"""Preset management tools for IEM mixes."""

import json
from pathlib import Path
from typing import Any


def get_presets_dir() -> Path:
    """Get the presets directory path."""
    return Path(__file__).parent.parent.parent.parent / "presets" / "members"


async def save_preset(
    client: Any,
    member_name: str,
    preset_name: str,
) -> dict:
    """
    Save current mix state as a preset for a band member.

    Args:
        client: REAPER HTTP client
        member_name: Band member name (e.g., "marek")
        preset_name: Preset name (e.g., "default", "rehearsal", "live")

    Returns:
        Status dict with success/error
    """
    try:
        # Get all tracks and their sends
        tracks_response = await client.get_tracks()

        # Build preset data - capture send levels for each input track
        preset_data = {
            "member": member_name,
            "name": preset_name,
            "sends": {}
        }

        for track in tracks_response:
            track_index = track.get("index", 0)
            track_name = track.get("name", "")
            send_count = track.get("send_count", 0)

            # Skip output buses
            if "inear" in track_name.lower():
                continue

            if send_count > 0:
                # Get send level for this track
                # For now, we just record that this track has sends
                preset_data["sends"][track_name] = {
                    "index": track_index,
                    "level": 1.0,  # Default - would need to query actual level
                    "muted": False
                }

        # Save to file
        presets_dir = get_presets_dir()
        presets_dir.mkdir(parents=True, exist_ok=True)

        member_dir = presets_dir / member_name.lower()
        member_dir.mkdir(exist_ok=True)

        preset_file = member_dir / f"{preset_name}.json"
        preset_file.write_text(json.dumps(preset_data, indent=2))

        return {
            "success": True,
            "message": f"Saved preset '{preset_name}' for {member_name}",
            "path": str(preset_file)
        }
    except Exception as e:
        return {
            "success": False,
            "error": str(e)
        }


async def load_preset(
    client: Any,
    member_name: str,
    preset_name: str,
) -> dict:
    """
    Load a preset and apply it to the current mix.

    Args:
        client: REAPER HTTP client
        member_name: Band member name (e.g., "marek")
        preset_name: Preset name to load

    Returns:
        Status dict with success/error
    """
    try:
        presets_dir = get_presets_dir()
        preset_file = presets_dir / member_name.lower() / f"{preset_name}.json"

        if not preset_file.exists():
            return {
                "success": False,
                "error": f"Preset '{preset_name}' not found for {member_name}"
            }

        preset_data = json.loads(preset_file.read_text())

        # Apply send levels from preset
        applied = []
        for track_name, send_info in preset_data.get("sends", {}).items():
            track_index = send_info.get("index")
            level = send_info.get("level", 1.0)

            if track_index:
                # Set send level (send 0 = first send to output bus)
                await client.send_command(f"SET/TRACK/{track_index}/SEND/0/VOL/{level}")
                applied.append(track_name)

        return {
            "success": True,
            "message": f"Loaded preset '{preset_name}' for {member_name}",
            "applied_to": applied
        }
    except Exception as e:
        return {
            "success": False,
            "error": str(e)
        }


async def list_presets(member_name: str) -> dict:
    """
    List available presets for a band member.

    Args:
        member_name: Band member name

    Returns:
        Dict with list of preset names
    """
    try:
        presets_dir = get_presets_dir()
        member_dir = presets_dir / member_name.lower()

        if not member_dir.exists():
            return {
                "member": member_name,
                "presets": []
            }

        presets = [f.stem for f in member_dir.glob("*.json")]

        return {
            "member": member_name,
            "presets": presets
        }
    except Exception as e:
        return {
            "error": str(e)
        }
