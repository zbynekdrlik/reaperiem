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
