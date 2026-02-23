# REAPER IEM Mixing System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build an MCP server that controls REAPER via HTTP Web API for personal monitor mixing, with Git-based version control via SSH.

**Architecture:** Python FastMCP server on dev machine connects to REAPER's web interface on iem.lan:8080 via HTTP. Git operations run on iem.lan via SSH. Band members access "More Me" web pages served by REAPER.

**Tech Stack:** Python 3.12, FastMCP, httpx (async HTTP), paramiko (SSH), PyYAML, REAPER Web API

---

## Phase 1: Repository & Project Structure

### Task 1.1: Create GitHub Repository

**Files:**

- Create: `README.md`
- Create: `.gitignore`
- Create: `CLAUDE.md`

**Step 1: Initialize git repository locally**

```bash
cd /home/newlevel/devel/reaperiem
git init
```

**Step 2: Create README.md**

````markdown
# REAPER IEM Mixing System

MCP server for controlling REAPER as a personal monitor (IEM) mixer for church band.

## Features

- HTTP-based control of REAPER tracks and sends
- Per-band-member "More Me" web interface
- Git version control of REAPER projects via SSH
- Claude Code integration via MCP

## Architecture

- **MCP Server**: Python + FastMCP on dev machine
- **REAPER**: Running on iem.lan with Web Interface enabled
- **Control**: HTTP Web API (port 8080)
- **Version Control**: Git on iem.lan via SSH

## Quick Start

```bash
# Install dependencies
pip install -e ./mcp/reaperiem_mcp

# Configure
cp config/reaper_config.yaml.example config/reaper_config.yaml
# Edit with your settings

# Run MCP server
python -m reaperiem_mcp.server
```
````

````

**Step 3: Create .gitignore**

```gitignore
# Python
__pycache__/
*.py[cod]
*$py.class
.venv/
venv/
*.egg-info/
dist/
build/

# REAPER
*.wav
*.mp3
*.flac
*.ogg
*.aif
*.aiff
peaks/
*.reapeaks
*.RPP-bak
*.RPP-UNDO

# IDE
.idea/
.vscode/
*.swp
*.swo

# Config with secrets
config/reaper_config.yaml

# OS
.DS_Store
Thumbs.db
````

**Step 4: Create CLAUDE.md**

```markdown
# REAPER IEM Mixing System

## Project Overview

MCP server for personal monitor mixing using REAPER's HTTP Web API.

## Key Commands

- `pytest` - Run tests
- `python -m reaperiem_mcp.server` - Run MCP server locally

## Architecture

- `mcp/reaperiem_mcp/` - FastMCP server code
- `config/` - YAML configuration files (reaper_config.yaml has secrets)
- `web/` - Custom REAPER web interface files
- `projects/` - REAPER project files (version controlled)

## REAPER HTTP API

Commands sent to `http://iem.lan:8080/_/command`:

- `SET/TRACK/index/VOL/value` - Set track volume (1.0 = 0dB)
- `SET/TRACK/x/SEND/y/VOL/value` - Set send volume
- `TRACK` or `TRACK/index` - Get track info
- `NTRACK` - Get track count

## Conventions

- Track names: UPPERCASE first word, lowercase second (e.g., "MAREK mic")
- Band member IDs: 1-indexed integers
- Dante outputs: Stereo pairs (L/R)
```

**Step 5: Create GitHub repo and push**

```bash
gh repo create reaperiem --private --source=. --remote=origin
git add README.md .gitignore CLAUDE.md
git commit -m "Initial project structure

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
git push -u origin main
```

**Step 6: Verify repo exists**

Run: `gh repo view newlevel/reaperiem`
Expected: Shows repository details

---

### Task 1.2: Create Configuration Files

**Files:**

- Create: `config/band_members.yaml`
- Create: `config/input_routing.yaml`
- Create: `config/reaper_config.yaml.example`

**Step 1: Create config directory**

```bash
mkdir -p /home/newlevel/devel/reaperiem/config
```

**Step 2: Create band_members.yaml**

```yaml
# Band member configuration for IEM routing
# Each member gets a stereo output pair for their in-ears

band_members:
  - id: 1
    name: "Marek"
    output_track_name: "MAREK inear"
    dante_output_L: 25
    dante_output_R: 26
```

**Step 3: Create input_routing.yaml**

```yaml
# Input track configuration
# Maps Dante inputs to REAPER tracks

input_tracks:
  - name: "MAREK mic"
    dante_input: 9
    default_level_db: 0.0
```

**Step 4: Create reaper_config.yaml.example**

```yaml
# REAPER connection configuration
# Copy to reaper_config.yaml and fill in values

reaper:
  host: "iem.lan"
  port: 8080
  # Optional HTTP Basic Auth (if enabled in REAPER)
  # username: ""
  # password: ""

ssh:
  host: "iem.lan"
  username: "newlevel"
  # Use SSH key authentication (recommended)
  key_path: "~/.ssh/id_rsa"
  # Or password (not recommended, use key instead)
  # password: ""

  # Git repository path on Windows
  repo_path: "C:\\Users\\newlevel\\Documents\\reaperiem"
```

**Step 5: Commit config files**

```bash
git add config/
git commit -m "feat: add configuration file templates

- band_members.yaml with initial Marek config
- input_routing.yaml with Marek mic input
- reaper_config.yaml.example template

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 1.3: Create MCP Server Package Structure

**Files:**

- Create: `mcp/reaperiem_mcp/__init__.py`
- Create: `mcp/reaperiem_mcp/server.py`
- Create: `mcp/reaperiem_mcp/tools/__init__.py`
- Create: `mcp/reaperiem_mcp/lib/__init__.py`
- Create: `mcp/pyproject.toml`

**Step 1: Create directory structure**

```bash
mkdir -p /home/newlevel/devel/reaperiem/mcp/reaperiem_mcp/{tools,lib}
```

**Step 2: Create mcp/pyproject.toml**

```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "reaperiem-mcp"
version = "0.1.0"
description = "MCP server for REAPER IEM mixing control"
requires-python = ">=3.10"
dependencies = [
    "fastmcp>=2.0.0",
    "httpx>=0.27.0",
    "paramiko>=3.4.0",
    "pyyaml>=6.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0.0",
    "pytest-asyncio>=0.23.0",
]

[tool.hatch.build.targets.wheel]
packages = ["reaperiem_mcp"]
```

**Step 3: Create mcp/reaperiem_mcp/**init**.py**

```python
"""REAPER IEM Mixing MCP Server."""

__version__ = "0.1.0"
```

**Step 4: Create mcp/reaperiem_mcp/server.py (minimal skeleton)**

```python
"""FastMCP server for REAPER IEM mixing control."""

from fastmcp import FastMCP

mcp = FastMCP(
    "REAPER IEM Mixer",
    instructions="Control REAPER for in-ear monitor mixing. Use track and send controls to adjust personal mixes.",
)


@mcp.tool
def ping() -> str:
    """Test connectivity to MCP server."""
    return "pong - REAPER IEM Mixer is running"


if __name__ == "__main__":
    mcp.run()
```

**Step 5: Create empty **init**.py files**

```python
# mcp/reaperiem_mcp/tools/__init__.py
"""MCP tool implementations."""

# mcp/reaperiem_mcp/lib/__init__.py
"""Support libraries for REAPER control."""
```

**Step 6: Commit MCP package structure**

```bash
git add mcp/
git commit -m "feat: add MCP server package structure

- FastMCP-based server skeleton
- pyproject.toml with dependencies
- tools/ and lib/ directories

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 2: REAPER HTTP Client Library

### Task 2.1: Create REAPER HTTP Client

**Files:**

- Create: `mcp/reaperiem_mcp/lib/reaper_http.py`
- Create: `mcp/reaperiem_mcp/tests/test_reaper_http.py`

**Step 1: Write the failing test**

```python
# mcp/reaperiem_mcp/tests/test_reaper_http.py
"""Tests for REAPER HTTP client."""

import pytest
from reaperiem_mcp.lib.reaper_http import ReaperHTTPClient


def test_build_command_url():
    """Test URL building for REAPER commands."""
    client = ReaperHTTPClient(host="iem.lan", port=8080)
    url = client._build_url("NTRACK")
    assert url == "http://iem.lan:8080/_/NTRACK"


def test_build_command_url_multiple_commands():
    """Test URL building with multiple commands."""
    client = ReaperHTTPClient(host="iem.lan", port=8080)
    url = client._build_url("NTRACK;TRACK/1")
    assert url == "http://iem.lan:8080/_/NTRACK;TRACK/1"


def test_parse_ntrack_response():
    """Test parsing NTRACK response."""
    client = ReaperHTTPClient(host="localhost", port=8080)
    response = "NTRACK\t5\n"
    result = client._parse_response(response)
    assert result == {"NTRACK": "5"}


def test_parse_track_response():
    """Test parsing TRACK response with tab-separated values."""
    client = ReaperHTTPClient(host="localhost", port=8080)
    response = "TRACK\t1\tMARE mic\t1.0\t0.0\t0\t0\t0\t0\t0\n"
    result = client._parse_response(response)
    assert "TRACK" in result
```

**Step 2: Run test to verify it fails**

Run: `cd /home/newlevel/devel/reaperiem/mcp && pip install -e . && pytest reaperiem_mcp/tests/test_reaper_http.py -v`
Expected: FAIL with "ModuleNotFoundError: No module named 'reaperiem_mcp.lib.reaper_http'"

**Step 3: Write minimal implementation**

```python
# mcp/reaperiem_mcp/lib/reaper_http.py
"""HTTP client for REAPER Web Interface API."""

from dataclasses import dataclass
from typing import Any

import httpx


@dataclass
class ReaperHTTPClient:
    """Client for REAPER's HTTP Web Interface.

    REAPER Web API uses commands sent to /_/command format.
    Responses are tab-separated values.
    """

    host: str
    port: int = 8080
    username: str | None = None
    password: str | None = None
    timeout: float = 5.0

    def _build_url(self, command: str) -> str:
        """Build URL for REAPER command."""
        return f"http://{self.host}:{self.port}/_/{command}"

    def _get_auth(self) -> httpx.BasicAuth | None:
        """Get HTTP Basic Auth if configured."""
        if self.username and self.password:
            return httpx.BasicAuth(self.username, self.password)
        return None

    def _parse_response(self, text: str) -> dict[str, Any]:
        """Parse tab-separated response from REAPER.

        Format: COMMAND\tvalue1\tvalue2\t...
        """
        result = {}
        for line in text.strip().split("\n"):
            if not line:
                continue
            parts = line.split("\t")
            if parts:
                cmd = parts[0]
                if len(parts) == 2:
                    result[cmd] = parts[1]
                elif len(parts) > 2:
                    result[cmd] = parts[1:]
                else:
                    result[cmd] = None
        return result

    async def send_command(self, command: str) -> dict[str, Any]:
        """Send command to REAPER and return parsed response."""
        url = self._build_url(command)
        async with httpx.AsyncClient(timeout=self.timeout) as client:
            response = await client.get(url, auth=self._get_auth())
            response.raise_for_status()
            return self._parse_response(response.text)

    async def get_track_count(self) -> int:
        """Get number of tracks in project."""
        result = await self.send_command("NTRACK")
        return int(result.get("NTRACK", 0))

    async def get_track(self, index: int) -> dict[str, Any]:
        """Get track info by index (1-based)."""
        result = await self.send_command(f"TRACK/{index}")
        return result

    async def get_all_tracks(self) -> list[dict[str, Any]]:
        """Get info for all tracks."""
        result = await self.send_command("TRACK")
        return result.get("TRACK", [])

    async def set_track_volume(self, index: int, volume: float) -> None:
        """Set track volume. 1.0 = 0dB."""
        await self.send_command(f"SET/TRACK/{index}/VOL/{volume}")

    async def set_send_volume(
        self, track_index: int, send_index: int, volume: float
    ) -> None:
        """Set send volume. 1.0 = 0dB."""
        await self.send_command(
            f"SET/TRACK/{track_index}/SEND/{send_index}/VOL/{volume}"
        )

    async def set_track_mute(self, index: int, mute: bool) -> None:
        """Set track mute state."""
        value = 1 if mute else 0
        await self.send_command(f"SET/TRACK/{index}/MUTE/{value}")

    async def set_track_solo(self, index: int, solo: bool) -> None:
        """Set track solo state."""
        value = 1 if solo else 0
        await self.send_command(f"SET/TRACK/{index}/SOLO/{value}")
```

**Step 4: Run test to verify it passes**

Run: `cd /home/newlevel/devel/reaperiem/mcp && pytest reaperiem_mcp/tests/test_reaper_http.py -v`
Expected: PASS

**Step 5: Create tests directory init**

```python
# mcp/reaperiem_mcp/tests/__init__.py
"""Tests for reaperiem_mcp."""
```

**Step 6: Commit**

```bash
git add mcp/reaperiem_mcp/lib/reaper_http.py mcp/reaperiem_mcp/tests/
git commit -m "feat: add REAPER HTTP client library

- ReaperHTTPClient with async HTTP support
- Track and send volume control
- Mute/solo controls
- Response parsing for tab-separated format

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 2.2: Create SSH Client for Git Operations

**Files:**

- Create: `mcp/reaperiem_mcp/lib/ssh_client.py`
- Create: `mcp/reaperiem_mcp/tests/test_ssh_client.py`

**Step 1: Write the failing test**

```python
# mcp/reaperiem_mcp/tests/test_ssh_client.py
"""Tests for SSH client."""

import pytest
from reaperiem_mcp.lib.ssh_client import SSHGitClient


def test_ssh_client_init():
    """Test SSH client initialization."""
    client = SSHGitClient(
        host="iem.lan",
        username="newlevel",
        repo_path=r"C:\Users\newlevel\Documents\reaperiem",
    )
    assert client.host == "iem.lan"
    assert client.username == "newlevel"
    assert client.repo_path == r"C:\Users\newlevel\Documents\reaperiem"


def test_build_git_command():
    """Test git command building with Windows path."""
    client = SSHGitClient(
        host="iem.lan",
        username="newlevel",
        repo_path=r"C:\Users\newlevel\Documents\reaperiem",
    )
    cmd = client._build_git_command("status")
    assert "cd" in cmd
    assert "git status" in cmd
```

**Step 2: Run test to verify it fails**

Run: `cd /home/newlevel/devel/reaperiem/mcp && pytest reaperiem_mcp/tests/test_ssh_client.py -v`
Expected: FAIL with "ModuleNotFoundError"

**Step 3: Write minimal implementation**

```python
# mcp/reaperiem_mcp/lib/ssh_client.py
"""SSH client for running git commands on remote Windows machine."""

from dataclasses import dataclass, field
from pathlib import Path

import paramiko


@dataclass
class SSHGitClient:
    """SSH client for git operations on iem.lan Windows PC."""

    host: str
    username: str
    repo_path: str
    key_path: str | None = None
    password: str | None = None
    port: int = 22
    _client: paramiko.SSHClient = field(default_factory=paramiko.SSHClient, repr=False)

    def __post_init__(self):
        """Initialize SSH client."""
        self._client.set_missing_host_key_policy(paramiko.AutoAddPolicy())

    def _build_git_command(self, git_cmd: str) -> str:
        """Build command to run git in repo directory.

        Uses PowerShell-style cd for Windows compatibility.
        """
        # Escape backslashes for shell
        escaped_path = self.repo_path.replace("\\", "\\\\")
        return f'cd "{escaped_path}" && git {git_cmd}'

    def _connect(self) -> None:
        """Establish SSH connection."""
        connect_kwargs = {
            "hostname": self.host,
            "port": self.port,
            "username": self.username,
        }

        if self.key_path:
            key_file = Path(self.key_path).expanduser()
            connect_kwargs["key_filename"] = str(key_file)
        elif self.password:
            connect_kwargs["password"] = self.password

        self._client.connect(**connect_kwargs)

    def _disconnect(self) -> None:
        """Close SSH connection."""
        self._client.close()

    def run_git(self, git_cmd: str) -> tuple[str, str, int]:
        """Run git command on remote and return (stdout, stderr, exit_code)."""
        cmd = self._build_git_command(git_cmd)
        try:
            self._connect()
            stdin, stdout, stderr = self._client.exec_command(cmd)
            exit_code = stdout.channel.recv_exit_status()
            return (
                stdout.read().decode("utf-8"),
                stderr.read().decode("utf-8"),
                exit_code,
            )
        finally:
            self._disconnect()

    def status(self) -> str:
        """Get git status."""
        stdout, stderr, code = self.run_git("status --porcelain")
        if code != 0:
            raise RuntimeError(f"git status failed: {stderr}")
        return stdout

    def add(self, paths: str = ".") -> None:
        """Stage files."""
        stdout, stderr, code = self.run_git(f"add {paths}")
        if code != 0:
            raise RuntimeError(f"git add failed: {stderr}")

    def commit(self, message: str) -> str:
        """Create commit with message."""
        # Escape quotes in message
        escaped_msg = message.replace('"', '\\"')
        stdout, stderr, code = self.run_git(f'commit -m "{escaped_msg}"')
        if code != 0:
            if "nothing to commit" in stdout or "nothing to commit" in stderr:
                return "Nothing to commit"
            raise RuntimeError(f"git commit failed: {stderr}")
        return stdout

    def push(self) -> str:
        """Push to remote."""
        stdout, stderr, code = self.run_git("push")
        if code != 0:
            raise RuntimeError(f"git push failed: {stderr}")
        return stdout

    def log(self, count: int = 5) -> str:
        """Get recent commits."""
        stdout, stderr, code = self.run_git(
            f"log --oneline -n {count}"
        )
        if code != 0:
            raise RuntimeError(f"git log failed: {stderr}")
        return stdout
```

**Step 4: Run test to verify it passes**

Run: `cd /home/newlevel/devel/reaperiem/mcp && pytest reaperiem_mcp/tests/test_ssh_client.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add mcp/reaperiem_mcp/lib/ssh_client.py mcp/reaperiem_mcp/tests/test_ssh_client.py
git commit -m "feat: add SSH client for remote git operations

- SSHGitClient using paramiko
- Git status, add, commit, push, log commands
- Windows path handling for iem.lan

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 2.3: Create Configuration Loader

**Files:**

- Create: `mcp/reaperiem_mcp/lib/config.py`
- Create: `mcp/reaperiem_mcp/tests/test_config.py`

**Step 1: Write the failing test**

```python
# mcp/reaperiem_mcp/tests/test_config.py
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
```

**Step 2: Run test to verify it fails**

Run: `cd /home/newlevel/devel/reaperiem/mcp && pytest reaperiem_mcp/tests/test_config.py -v`
Expected: FAIL

**Step 3: Write minimal implementation**

```python
# mcp/reaperiem_mcp/lib/config.py
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

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Config":
        """Create Config from dictionary."""
        reaper = data.get("reaper", {})
        ssh = data.get("ssh", {})

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
```

**Step 4: Run test to verify it passes**

Run: `cd /home/newlevel/devel/reaperiem/mcp && pytest reaperiem_mcp/tests/test_config.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add mcp/reaperiem_mcp/lib/config.py mcp/reaperiem_mcp/tests/test_config.py
git commit -m "feat: add configuration loader

- Config dataclass for REAPER and SSH settings
- YAML file loading
- Standard config directory search

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 3: MCP Tools Implementation

### Task 3.1: Implement Track Tools

**Files:**

- Create: `mcp/reaperiem_mcp/tools/tracks.py`
- Modify: `mcp/reaperiem_mcp/server.py`

**Step 1: Write tracks.py**

```python
# mcp/reaperiem_mcp/tools/tracks.py
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
```

**Step 2: Update server.py with track tools**

```python
# mcp/reaperiem_mcp/server.py
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
```

**Step 3: Commit**

```bash
git add mcp/reaperiem_mcp/tools/tracks.py mcp/reaperiem_mcp/server.py
git commit -m "feat: add track control MCP tools

- list_tracks, get_track tools
- set_track_volume with dB conversion
- mute_track, solo_track controls

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 3.2: Implement Send/Mix Tools

**Files:**

- Create: `mcp/reaperiem_mcp/tools/mix.py`
- Modify: `mcp/reaperiem_mcp/server.py`

**Step 1: Write mix.py**

```python
# mcp/reaperiem_mcp/tools/mix.py
"""MCP tools for send/mix control - the core of IEM mixing."""

from typing import Any

from ..lib.reaper_http import ReaperHTTPClient


async def set_send_level(
    client: ReaperHTTPClient,
    track_index: int,
    send_index: int,
    level_db: float,
) -> str:
    """Set send level from track to output bus.

    This is the core function for IEM mixing - each band member's
    output bus receives sends from all input tracks.
    """
    # Convert dB to linear
    linear = 10 ** (level_db / 20)
    await client.set_send_volume(track_index, send_index, linear)
    return f"Send from track {track_index} to send {send_index} set to {level_db}dB"


async def adjust_send_level(
    client: ReaperHTTPClient,
    track_index: int,
    send_index: int,
    adjustment_db: float,
) -> str:
    """Adjust send level relatively by dB amount.

    Uses REAPER's relative adjustment feature (+ prefix).
    """
    # REAPER accepts +value or -value for relative adjustment
    sign = "+" if adjustment_db >= 0 else ""
    await client.send_command(
        f"SET/TRACK/{track_index}/SEND/{send_index}/VOL/{sign}{adjustment_db}"
    )
    return f"Send adjusted by {adjustment_db}dB"


def db_to_linear(db: float) -> float:
    """Convert dB to linear gain."""
    return 10 ** (db / 20)


def linear_to_db(linear: float) -> float:
    """Convert linear gain to dB."""
    import math
    if linear <= 0:
        return float("-inf")
    return 20 * math.log10(linear)
```

**Step 2: Add send tools to server.py**

Add after solo_track tool:

```python
from .tools import mix

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
```

**Step 3: Commit**

```bash
git add mcp/reaperiem_mcp/tools/mix.py mcp/reaperiem_mcp/server.py
git commit -m "feat: add send/mix control MCP tools

- set_send_level for absolute control
- adjust_send_level for relative adjustments
- dB to linear conversion utilities

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 3.3: Implement Git Tools

**Files:**

- Create: `mcp/reaperiem_mcp/tools/git.py`
- Modify: `mcp/reaperiem_mcp/server.py`

**Step 1: Write git.py**

```python
# mcp/reaperiem_mcp/tools/git.py
"""MCP tools for git operations on iem.lan via SSH."""

from ..lib.ssh_client import SSHGitClient


def git_status(client: SSHGitClient) -> str:
    """Get git status of REAPER project repository on iem.lan."""
    status = client.status()
    if not status.strip():
        return "Working directory clean - no changes"
    return f"Changed files:\n{status}"


def git_commit(client: SSHGitClient, message: str) -> str:
    """Commit changes to REAPER project on iem.lan."""
    # Stage all changes
    client.add(".")
    # Commit
    result = client.commit(message)
    return result


def git_push(client: SSHGitClient) -> str:
    """Push commits from iem.lan to GitHub."""
    result = client.push()
    return result if result.strip() else "Pushed successfully"


def git_log(client: SSHGitClient, count: int = 5) -> str:
    """Show recent commit history."""
    return client.log(count)
```

**Step 2: Add git tools and SSH client to server.py**

Add imports and SSH client:

```python
from .lib.ssh_client import SSHGitClient
from .tools import git

# Add SSH client global
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
```

Add tools:

```python
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
```

**Step 3: Commit**

```bash
git add mcp/reaperiem_mcp/tools/git.py mcp/reaperiem_mcp/server.py
git commit -m "feat: add git MCP tools for iem.lan

- git_status to check changed files
- git_commit to save project changes
- git_push to sync to GitHub
- git_log to view history

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 3.4: Implement Band Member Tools

**Files:**

- Create: `mcp/reaperiem_mcp/tools/band.py`
- Modify: `mcp/reaperiem_mcp/server.py`

**Step 1: Write band.py**

```python
# mcp/reaperiem_mcp/tools/band.py
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
    path = Path(config_dir) / "input_routing.yaml"
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
    path = Path(config_dir) / "input_routing.yaml"
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
```

**Step 2: Add band tools to server.py**

```python
from .tools import band

# Config directory (relative to project root)
def get_config_dir() -> Path:
    """Get config directory path."""
    # Look relative to this file or use environment
    return Path(__file__).parent.parent.parent.parent / "config"


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
```

**Step 3: Add Path import at top of server.py**

```python
from pathlib import Path
```

**Step 4: Commit**

```bash
git add mcp/reaperiem_mcp/tools/band.py mcp/reaperiem_mcp/server.py
git commit -m "feat: add band member management MCP tools

- list_band_members, list_input_tracks
- add_band_member with naming convention
- add_input_track for input routing
- YAML-based persistence

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 4: Clone Repository on iem.lan

### Task 4.1: Clone and Configure Git on Windows

**This task requires manual steps on the iem.lan Windows PC.**

**Step 1: Install Git on Windows (if not installed)**

On iem.lan, download and install Git for Windows from https://git-scm.com/download/win

**Step 2: Open PowerShell and clone repository**

```powershell
cd C:\Users\newlevel\Documents
git clone https://github.com/newlevel/reaperiem.git
cd reaperiem
```

**Step 3: Configure git credentials**

```powershell
git config user.name "newlevel"
git config user.email "your-email@example.com"
git config credential.helper manager
```

**Step 4: Test SSH access from dev machine**

From dev machine:

```bash
ssh newlevel@iem.lan "cd C:\\Users\\newlevel\\Documents\\reaperiem && git status"
```

Expected: Shows git status output

---

## Phase 5: Web Interface

### Task 5.1: Create IEM Mixer Web Interface

**Files:**

- Create: `web/reaper_interface/iem_mixer.html`

**Step 1: Create web directory**

```bash
mkdir -p /home/newlevel/devel/reaperiem/web/reaper_interface
```

**Step 2: Write iem_mixer.html**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta
      name="viewport"
      content="width=device-width, initial-scale=1.0, user-scalable=no"
    />
    <title>IEM Mixer</title>
    <style>
      * {
        box-sizing: border-box;
        touch-action: manipulation;
      }

      body {
        font-family:
          -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
        background: #1a1a2e;
        color: #eee;
        margin: 0;
        padding: 10px;
        min-height: 100vh;
      }

      .header {
        text-align: center;
        padding: 10px;
        border-bottom: 1px solid #333;
        margin-bottom: 20px;
      }

      .header h1 {
        margin: 0;
        font-size: 1.5em;
        color: #4ecdc4;
      }

      .member-name {
        font-size: 0.9em;
        color: #888;
      }

      .channels {
        display: flex;
        flex-direction: column;
        gap: 15px;
      }

      .channel {
        background: #16213e;
        border-radius: 10px;
        padding: 15px;
        display: flex;
        align-items: center;
        gap: 15px;
      }

      .channel-name {
        min-width: 100px;
        font-weight: bold;
        font-size: 0.9em;
      }

      .slider-container {
        flex: 1;
        display: flex;
        align-items: center;
        gap: 10px;
      }

      .slider {
        flex: 1;
        -webkit-appearance: none;
        height: 40px;
        background: #0f3460;
        border-radius: 20px;
        outline: none;
      }

      .slider::-webkit-slider-thumb {
        -webkit-appearance: none;
        width: 50px;
        height: 50px;
        background: #4ecdc4;
        border-radius: 50%;
        cursor: pointer;
      }

      .slider::-moz-range-thumb {
        width: 50px;
        height: 50px;
        background: #4ecdc4;
        border-radius: 50%;
        cursor: pointer;
        border: none;
      }

      .level-display {
        min-width: 60px;
        text-align: right;
        font-family: monospace;
        font-size: 1.1em;
      }

      .mute-btn {
        width: 50px;
        height: 50px;
        border: none;
        border-radius: 10px;
        font-weight: bold;
        cursor: pointer;
        font-size: 0.8em;
      }

      .mute-btn.off {
        background: #333;
        color: #888;
      }

      .mute-btn.on {
        background: #e63946;
        color: white;
      }

      .presets {
        margin-top: 20px;
        display: flex;
        gap: 10px;
        flex-wrap: wrap;
      }

      .preset-btn {
        flex: 1;
        min-width: 80px;
        padding: 15px;
        border: none;
        border-radius: 10px;
        background: #0f3460;
        color: #eee;
        font-size: 1em;
        cursor: pointer;
      }

      .preset-btn:active {
        background: #4ecdc4;
        color: #1a1a2e;
      }

      .status {
        position: fixed;
        bottom: 10px;
        left: 10px;
        right: 10px;
        padding: 10px;
        background: #333;
        border-radius: 5px;
        text-align: center;
        font-size: 0.8em;
      }

      .status.connected {
        background: #1d3557;
      }

      .status.error {
        background: #e63946;
      }
    </style>
  </head>
  <body>
    <div class="header">
      <h1>IEM Mixer</h1>
      <div class="member-name" id="memberName">Loading...</div>
    </div>

    <div class="channels" id="channels">
      <!-- Channels populated by JavaScript -->
    </div>

    <div class="presets">
      <button class="preset-btn" onclick="loadPreset('default')">
        Default
      </button>
      <button class="preset-btn" onclick="loadPreset('more_me')">
        More Me
      </button>
      <button class="preset-btn" onclick="loadPreset('less_me')">
        Less Me
      </button>
      <button class="preset-btn" onclick="savePreset()">Save</button>
    </div>

    <div class="status" id="status">Connecting...</div>

    <script src="/main.js"></script>
    <script>
      // Get member ID from URL: /member/1, /member/2, etc.
      const pathParts = window.location.pathname.split("/");
      const memberId = parseInt(pathParts[pathParts.length - 1]) || 1;

      let tracks = [];
      let memberSendIndex = memberId; // Assumes send index matches member ID

      // Initialize REAPER web interface
      wwr_start();

      // Request track data periodically
      function updateTracks() {
        wwr_req_recur("NTRACK;TRACK", 500);
      }

      // Handle responses from REAPER
      wwr_onreply = function (response) {
        const lines = response.split("\n");
        let trackCount = 0;
        const newTracks = [];

        for (const line of lines) {
          const parts = line.split("\t");
          if (parts[0] === "NTRACK") {
            trackCount = parseInt(parts[1]);
          } else if (parts[0] === "TRACK" && parts.length > 2) {
            newTracks.push({
              index: parseInt(parts[1]),
              name: parts[2],
              volume: parseFloat(parts[3]) || 1.0,
              pan: parseFloat(parts[4]) || 0,
              mute: parseInt(parts[5]) || 0,
            });
          }
        }

        if (newTracks.length > 0) {
          tracks = newTracks;
          renderChannels();
          updateStatus("connected", "Connected");
        }
      };

      function renderChannels() {
        const container = document.getElementById("channels");
        container.innerHTML = "";

        // Filter to show only input tracks (not output buses)
        const inputTracks = tracks.filter((t) => !t.name.includes("inear"));

        for (const track of inputTracks) {
          const div = document.createElement("div");
          div.className = "channel";
          div.innerHTML = `
                    <div class="channel-name">${track.name}</div>
                    <div class="slider-container">
                        <input type="range" class="slider"
                               min="0" max="100" value="${volumeToSlider(track.volume)}"
                               data-track="${track.index}"
                               onchange="setVolume(${track.index}, this.value)"
                               oninput="updateDisplay(${track.index}, this.value)">
                        <div class="level-display" id="level-${track.index}">
                            ${volumeToDb(track.volume)}
                        </div>
                    </div>
                    <button class="mute-btn ${track.mute ? "on" : "off"}"
                            onclick="toggleMute(${track.index})">
                        M
                    </button>
                `;
          container.appendChild(div);
        }
      }

      function volumeToSlider(vol) {
        // Convert linear 0-2 to slider 0-100 (log scale)
        if (vol <= 0) return 0;
        const db = 20 * Math.log10(vol);
        // Map -60dB to +6dB -> 0 to 100
        return Math.max(0, Math.min(100, ((db + 60) * 100) / 66));
      }

      function sliderToVolume(slider) {
        // Convert slider 0-100 to linear volume
        const db = (slider * 66) / 100 - 60;
        return Math.pow(10, db / 20);
      }

      function volumeToDb(vol) {
        if (vol <= 0) return "-∞ dB";
        const db = 20 * Math.log10(vol);
        return db.toFixed(1) + " dB";
      }

      function setVolume(trackIndex, sliderValue) {
        const vol = sliderToVolume(sliderValue);
        // Set send level from this track to member's output
        wwr_req(`SET/TRACK/${trackIndex}/SEND/${memberSendIndex}/VOL/${vol}`);
      }

      function updateDisplay(trackIndex, sliderValue) {
        const vol = sliderToVolume(sliderValue);
        document.getElementById(`level-${trackIndex}`).textContent =
          volumeToDb(vol);
      }

      function toggleMute(trackIndex) {
        wwr_req(`SET/TRACK/${trackIndex}/SEND/${memberSendIndex}/MUTE/-1`);
      }

      function loadPreset(name) {
        // TODO: Load preset from server
        updateStatus("connected", `Loading ${name}...`);
      }

      function savePreset() {
        // TODO: Save current mix as preset
        updateStatus("connected", "Saving...");
      }

      function updateStatus(type, message) {
        const status = document.getElementById("status");
        status.className = "status " + type;
        status.textContent = message;
      }

      // Set member name
      document.getElementById("memberName").textContent = `Member ${memberId}`;

      // Start updates
      updateTracks();
    </script>
  </body>
</html>
```

**Step 3: Commit web interface**

```bash
git add web/
git commit -m "feat: add IEM mixer web interface

- Mobile-friendly touch interface
- Per-member URL routing (/member/1, /member/2)
- Large sliders for volume control
- Mute buttons and preset system

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 5.2: Create Deploy Script

**Files:**

- Create: `scripts/deploy.sh`

**Step 1: Write deploy.sh**

```bash
#!/bin/bash
# Deploy web interface to REAPER on iem.lan

set -e

REMOTE_USER="newlevel"
REMOTE_HOST="iem.lan"
REMOTE_REAPER_WEB="/c/Program Files/REAPER (x64)/Data/web_interface"
LOCAL_WEB="./web/reaper_interface/"

echo "Deploying web interface to iem.lan..."

# Use rsync over SSH
rsync -avz --progress \
    "$LOCAL_WEB" \
    "${REMOTE_USER}@${REMOTE_HOST}:${REMOTE_REAPER_WEB}/iem_mixer/"

echo "Deploy complete!"
echo "Access at: http://iem.lan:8080/iem_mixer/iem_mixer.html"
```

**Step 2: Make executable and commit**

```bash
chmod +x scripts/deploy.sh
git add scripts/
git commit -m "feat: add web interface deploy script

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 6: Final Integration

### Task 6.1: Create Local Config and Test

**Step 1: Create local reaper_config.yaml**

```bash
cp config/reaper_config.yaml.example config/reaper_config.yaml
```

Edit with actual values:

```yaml
reaper:
  host: "iem.lan"
  port: 8080

ssh:
  host: "iem.lan"
  username: "newlevel"
  key_path: "~/.ssh/id_rsa"
  repo_path: "C:\\Users\\newlevel\\Documents\\reaperiem"
```

**Step 2: Install MCP server locally**

```bash
cd /home/newlevel/devel/reaperiem/mcp
pip install -e .[dev]
```

**Step 3: Run tests**

```bash
pytest -v
```

**Step 4: Test MCP server startup**

```bash
python -m reaperiem_mcp.server
```

Expected: Server starts without errors

---

### Task 6.2: Push All Changes

**Step 1: Final commit and push**

```bash
git add -A
git status
# If there are uncommitted changes:
git commit -m "chore: final cleanup and documentation

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
git push
```

**Step 2: Pull on iem.lan**

From dev machine:

```bash
ssh newlevel@iem.lan "cd C:\\Users\\newlevel\\Documents\\reaperiem && git pull"
```

---

## Verification Checklist

- [ ] GitHub repository exists and is private
- [ ] SSH to iem.lan works from dev machine
- [ ] REAPER web interface responds at http://iem.lan:8080/
- [ ] MCP server starts without errors
- [ ] `ping` tool returns "pong"
- [ ] `list_tracks` returns track data from REAPER
- [ ] `set_send_level` changes send levels in REAPER
- [ ] `git_status` shows iem.lan repo status
- [ ] `git_commit` and `git_push` work
- [ ] Web interface loads at http://iem.lan:8080/iem_mixer/iem_mixer.html
- [ ] Volume sliders control sends in REAPER

---

## Post-Implementation

After completing the plan:

1. Add more band members to config
2. Create REAPER project with proper track layout
3. Test with actual band members on mobile devices
4. Add preset save/load functionality
5. Document operational procedures
