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
