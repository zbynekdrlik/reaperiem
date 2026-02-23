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
