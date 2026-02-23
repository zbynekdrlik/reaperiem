"""Configuration loading for REAPER IEM MCP server."""

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml


@dataclass
class Config:
    """Configuration for REAPER IEM MCP server."""

    # REAPER connection
    reaper_host: str
    reaper_port: int = 8080
    reaper_username: str | None = None
    reaper_password: str | None = None

    # SSH connection
    ssh_host: str = ""
    ssh_username: str = ""
    ssh_repo_path: str = ""
    ssh_key_path: str | None = None
    ssh_password: str | None = None
    ssh_port: int = 22

    # ReaScript action IDs
    action_set_hardware_output: str = ""

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Config":
        """Create Config from dictionary."""
        reaper = data.get("reaper", {})
        ssh = data.get("ssh", {})
        actions = data.get("actions", {})

        return cls(
            reaper_host=reaper.get("host", "localhost"),
            reaper_port=reaper.get("port", 8080),
            reaper_username=reaper.get("username"),
            reaper_password=reaper.get("password"),
            ssh_host=ssh.get("host", ""),
            ssh_username=ssh.get("username", ""),
            ssh_repo_path=ssh.get("repo_path", ""),
            ssh_key_path=ssh.get("key_path"),
            ssh_password=ssh.get("password"),
            ssh_port=ssh.get("port", 22),
            action_set_hardware_output=actions.get("set_hardware_output", ""),
        )

    @classmethod
    def from_yaml(cls, path: Path | str) -> "Config":
        """Load config from YAML file."""
        path = Path(path)
        with path.open() as f:
            data = yaml.safe_load(f)
        return cls.from_dict(data)


def load_config(config_dir: Path | str | None = None) -> Config:
    """Load configuration from standard location.

    Looks for reaper_config.yaml in:
    1. Provided config_dir
    2. ./config/
    3. ../config/ (from mcp package)
    """
    search_paths = []

    if config_dir:
        search_paths.append(Path(config_dir))

    # Standard locations
    search_paths.extend([
        Path("config"),
        Path(__file__).parent.parent.parent.parent / "config",
    ])

    for base in search_paths:
        config_file = base / "reaper_config.yaml"
        if config_file.exists():
            return Config.from_yaml(config_file)

    raise FileNotFoundError(
        f"reaper_config.yaml not found in: {[str(p) for p in search_paths]}"
    )
