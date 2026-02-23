"""Tests for get_track_meter tool."""

import math

import pytest
from unittest.mock import AsyncMock

from reaperiem_mcp.lib.reaper_http import ReaperHTTPClient
from reaperiem_mcp.tools.mix import get_track_meter, linear_to_db


@pytest.fixture
def mock_client():
    """Create a mock ReaperHTTPClient."""
    client = ReaperHTTPClient(host="iem.lan", port=8080)
    client.send_command = AsyncMock(return_value={})
    return client


# --- linear_to_db edge cases ---


def test_linear_to_db_unity():
    """1.0 linear should be 0.0 dB."""
    assert linear_to_db(1.0) == 0.0


def test_linear_to_db_half():
    """0.5 linear should be approximately -6.02 dB."""
    assert abs(linear_to_db(0.5) - (-6.0206)) < 0.01


def test_linear_to_db_zero():
    """0.0 linear should return -inf dB."""
    assert linear_to_db(0.0) == float("-inf")


def test_linear_to_db_negative():
    """Negative linear values should return -inf dB."""
    assert linear_to_db(-0.1) == float("-inf")


# --- get_track_meter tests ---


@pytest.mark.asyncio
async def test_get_track_meter_stereo(mock_client):
    """Should return peak and RMS for both channels of a stereo track."""
    # REAPER returns meter data as part of TRACK response
    # Format: TRACK index name flags vol pan ... peak_l peak_r
    # Simulate get_track returning meter values
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": ["1", "MAREK mic", "0", "1.0", "0.5",
                   "0", "0", "0", "0", "0", "0", "0",
                   "0.5", "0.25"],  # peak_l=0.5, peak_r=0.25
    })
    result = await get_track_meter(mock_client, 1)

    assert "peak_l" in result
    assert "peak_r" in result
    assert isinstance(result["peak_l"], float)
    assert isinstance(result["peak_r"], float)
    # 0.5 linear -> ~-6.02 dB
    assert abs(result["peak_l"] - 20 * math.log10(0.5)) < 0.01
    # 0.25 linear -> ~-12.04 dB
    assert abs(result["peak_r"] - 20 * math.log10(0.25)) < 0.01


@pytest.mark.asyncio
async def test_get_track_meter_returns_all_fields(mock_client):
    """Should return peak_l, peak_r, rms_l, rms_r fields."""
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": ["1", "DRUMS L", "0", "1.0", "0.5",
                   "0", "0", "0", "0", "0", "0", "0",
                   "0.8", "0.7"],
    })
    result = await get_track_meter(mock_client, 1)

    expected_keys = {"peak_l", "peak_r", "rms_l", "rms_r", "track_index"}
    assert set(result.keys()) == expected_keys


@pytest.mark.asyncio
async def test_get_track_meter_mono_track(mock_client):
    """Mono track should return same value for L and R."""
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": ["3", "CLICK", "0", "1.0", "0.0",
                   "0", "0", "0", "0", "0", "0", "0",
                   "0.6"],  # Only one meter value -> mono
    })
    result = await get_track_meter(mock_client, 3)

    # Mono: L and R should be the same
    assert result["peak_l"] == result["peak_r"]
    assert result["rms_l"] == result["rms_r"]


@pytest.mark.asyncio
async def test_get_track_meter_silence(mock_client):
    """Silent track should return -inf dB for all values."""
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": ["2", "UNUSED", "0", "1.0", "0.0",
                   "0", "0", "0", "0", "0", "0", "0",
                   "0.0", "0.0"],
    })
    result = await get_track_meter(mock_client, 2)

    assert result["peak_l"] == float("-inf")
    assert result["peak_r"] == float("-inf")
    assert result["rms_l"] == float("-inf")
    assert result["rms_r"] == float("-inf")


@pytest.mark.asyncio
async def test_get_track_meter_full_scale(mock_client):
    """Full scale (1.0 linear) should return 0.0 dB."""
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": ["1", "LOUD", "0", "1.0", "0.0",
                   "0", "0", "0", "0", "0", "0", "0",
                   "1.0", "1.0"],
    })
    result = await get_track_meter(mock_client, 1)

    assert result["peak_l"] == 0.0
    assert result["peak_r"] == 0.0


@pytest.mark.asyncio
async def test_get_track_meter_track_not_found(mock_client):
    """Should raise ValueError when track doesn't exist."""
    mock_client.send_command = AsyncMock(return_value={})
    with pytest.raises(ValueError, match="Track 99 not found"):
        await get_track_meter(mock_client, 99)


@pytest.mark.asyncio
async def test_get_track_meter_correct_track_index(mock_client):
    """Should pass correct track index to REAPER API."""
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": ["5", "BASS", "0", "1.0", "0.0",
                   "0", "0", "0", "0", "0", "0", "0",
                   "0.3", "0.3"],
    })
    result = await get_track_meter(mock_client, 5)

    mock_client.send_command.assert_called_once_with("TRACK/5")
    assert result["track_index"] == 5


@pytest.mark.asyncio
async def test_get_track_meter_rms_less_than_peak(mock_client):
    """RMS values should be less than or equal to peak values."""
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": ["1", "GUITAR", "0", "1.0", "0.0",
                   "0", "0", "0", "0", "0", "0", "0",
                   "0.8", "0.7"],
    })
    result = await get_track_meter(mock_client, 1)

    # RMS should be <= peak (approximation: RMS ~= peak * 0.707 for sine)
    assert result["rms_l"] <= result["peak_l"]
    assert result["rms_r"] <= result["peak_r"]


@pytest.mark.asyncio
async def test_get_track_meter_no_meter_data(mock_client):
    """Track exists but has no meter fields should return -inf."""
    # Track data too short to have meter values
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": ["1", "SHORT", "0", "1.0", "0.5"],
    })
    result = await get_track_meter(mock_client, 1)

    assert result["peak_l"] == float("-inf")
    assert result["peak_r"] == float("-inf")
    assert result["rms_l"] == float("-inf")
    assert result["rms_r"] == float("-inf")


# --- ReaperHTTPClient.get_track_meter tests ---


def test_reaper_client_has_get_track_meter():
    """ReaperHTTPClient should have a get_track_meter method."""
    client = ReaperHTTPClient(host="localhost", port=8080)
    assert hasattr(client, "get_track_meter")
    assert callable(client.get_track_meter)
