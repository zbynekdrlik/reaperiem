"""Tests for configuration loading."""

import pytest
from pathlib import Path
from reaperiem_mcp.lib.config import Config, load_config


def test_config_from_dict():
    """Test creating Config from dictionary."""
    data = {
        "reaper": {"host": "iem.lan", "port": 8080},
        "ssh": {
            "host": "iem.lan",
            "username": "newlevel",
            "repo_path": r"C:\Users\newlevel\Documents\reaperiem",
        },
    }
    config = Config.from_dict(data)
    assert config.reaper_host == "iem.lan"
    assert config.reaper_port == 8080
    assert config.ssh_username == "newlevel"


def test_config_defaults():
    """Test Config uses sensible defaults."""
    data = {
        "reaper": {"host": "iem.lan"},
        "ssh": {
            "host": "iem.lan",
            "username": "newlevel",
            "repo_path": r"C:\repo",
        },
    }
    config = Config.from_dict(data)
    assert config.reaper_port == 8080  # default
