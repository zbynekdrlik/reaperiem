"""Tests for mix tools (send level, pan, and mute control)."""

import math
import pytest
import pytest_asyncio
from unittest.mock import AsyncMock, patch

from reaperiem_mcp.lib.reaper_http import ReaperHTTPClient
from reaperiem_mcp.tools.mix import (
    set_send_pan,
    set_send_mute,
    get_track_meter,
    cb_to_linear,
    db10_to_linear,
)


@pytest.fixture
def mock_client():
    """Create a mock ReaperHTTPClient."""
    client = ReaperHTTPClient(host="iem.lan", port=8080)
    client.send_command = AsyncMock(return_value={})
    return client


# --- set_send_pan tests ---
# REAPER SET PAN uses -1.0..1.0 natively (verified empirically 2026-03-01)


@pytest.mark.asyncio
async def test_set_send_pan_center(mock_client):
    """Pan 0.0 (center) should pass through as REAPER value 0.0."""
    mock_client.set_send_pan = AsyncMock()
    result = await set_send_pan(mock_client, 1, 0, 0.0)
    mock_client.set_send_pan.assert_called_once_with(1, 0, 0.0)
    assert "0.0" in result or "center" in result.lower()


@pytest.mark.asyncio
async def test_set_send_pan_hard_left(mock_client):
    """Pan -1.0 (hard left) should pass through as REAPER value -1.0."""
    mock_client.set_send_pan = AsyncMock()
    result = await set_send_pan(mock_client, 1, 0, -1.0)
    mock_client.set_send_pan.assert_called_once_with(1, 0, -1.0)


@pytest.mark.asyncio
async def test_set_send_pan_hard_right(mock_client):
    """Pan 1.0 (hard right) should pass through as REAPER value 1.0."""
    mock_client.set_send_pan = AsyncMock()
    result = await set_send_pan(mock_client, 1, 0, 1.0)
    mock_client.set_send_pan.assert_called_once_with(1, 0, 1.0)


@pytest.mark.asyncio
async def test_set_send_pan_partial(mock_client):
    """Pan 0.5 should pass through as REAPER value 0.5."""
    mock_client.set_send_pan = AsyncMock()
    result = await set_send_pan(mock_client, 3, 2, 0.5)
    mock_client.set_send_pan.assert_called_once_with(3, 2, 0.5)
    assert "0.5" in result


@pytest.mark.asyncio
async def test_set_send_pan_negative_partial(mock_client):
    """Pan -0.5 should pass through as REAPER value -0.5."""
    mock_client.set_send_pan = AsyncMock()
    result = await set_send_pan(mock_client, 2, 1, -0.5)
    mock_client.set_send_pan.assert_called_once_with(2, 1, -0.5)


@pytest.mark.asyncio
async def test_set_send_pan_out_of_range_high(mock_client):
    """Pan > 1.0 should raise ValueError."""
    with pytest.raises(ValueError, match="between -1.0 and 1.0"):
        await set_send_pan(mock_client, 1, 0, 1.5)


@pytest.mark.asyncio
async def test_set_send_pan_out_of_range_low(mock_client):
    """Pan < -1.0 should raise ValueError."""
    with pytest.raises(ValueError, match="between -1.0 and 1.0"):
        await set_send_pan(mock_client, 1, 0, -1.5)


@pytest.mark.asyncio
async def test_set_send_pan_returns_descriptive_message(mock_client):
    """Result message should include track, send, and pan info."""
    mock_client.set_send_pan = AsyncMock()
    result = await set_send_pan(mock_client, 5, 3, 0.0)
    assert "track 5" in result.lower()
    assert "send 3" in result.lower()


# --- set_send_mute tests ---


@pytest.mark.asyncio
async def test_set_send_mute_mute(mock_client):
    """Muting a send should send MUTE/1 command."""
    result = await set_send_mute(mock_client, 1, 0, True)
    mock_client.send_command.assert_called_once_with(
        "SET/TRACK/1/SEND/0/MUTE/1"
    )
    assert "mute" in result.lower()


@pytest.mark.asyncio
async def test_set_send_mute_unmute(mock_client):
    """Unmuting a send should send MUTE/0 command."""
    result = await set_send_mute(mock_client, 1, 0, False)
    mock_client.send_command.assert_called_once_with(
        "SET/TRACK/1/SEND/0/MUTE/0"
    )
    assert "unmute" in result.lower()


@pytest.mark.asyncio
async def test_set_send_mute_different_indices(mock_client):
    """Should use correct track and send indices in the command."""
    result = await set_send_mute(mock_client, 3, 2, True)
    mock_client.send_command.assert_called_once_with(
        "SET/TRACK/3/SEND/2/MUTE/1"
    )


@pytest.mark.asyncio
async def test_set_send_mute_returns_descriptive_message(mock_client):
    """Result message should include track and send info."""
    result = await set_send_mute(mock_client, 5, 3, True)
    assert "track 5" in result.lower()
    assert "send 3" in result.lower()


@pytest.mark.asyncio
async def test_set_send_mute_unmute_message(mock_client):
    """Unmute result message should say unmuted, not muted."""
    result = await set_send_mute(mock_client, 2, 1, False)
    assert "unmute" in result.lower()
    assert "track 2" in result.lower()
    assert "send 1" in result.lower()


# --- get_track_meter tests (dB*10 values) ---


@pytest.mark.asyncio
async def test_get_track_meter_with_signal(mock_client):
    """Meter with signal: dB*10 = -231 means -23.1 dB, -350 means -35.0 dB."""
    # TRACK response after keyword strip: [idx, name, flags, vol, pan, VU_L, VU_R, ...]
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": [1, "PETKA mic", 0, 1.0, 0.0, -231, -350, 2, 3, 5, 0, 0, 0]
    })
    result = await get_track_meter(mock_client, 1)
    assert result["track_index"] == 1
    assert abs(result["peak_l"] - (-23.1)) < 0.01
    assert abs(result["peak_r"] - (-35.0)) < 0.01
    # RMS should be peak - 3 dB
    assert abs(result["rms_l"] - (-26.1)) < 0.01
    assert abs(result["rms_r"] - (-38.0)) < 0.01


@pytest.mark.asyncio
async def test_get_track_meter_silence(mock_client):
    """Meter at floor (-1500 dB*10 = -150 dB) should report -inf."""
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": [1, "PETKA mic", 0, 1.0, 0.0, -1500, -1500, 2, 3, 5, 0, 0, 0]
    })
    result = await get_track_meter(mock_client, 1)
    assert result["peak_l"] == float("-inf")
    assert result["peak_r"] == float("-inf")
    assert result["rms_l"] == float("-inf")
    assert result["rms_r"] == float("-inf")


@pytest.mark.asyncio
async def test_get_track_meter_unity(mock_client):
    """Meter at 0 dB (dB*10 = 0) should report 0.0 dB."""
    mock_client.send_command = AsyncMock(return_value={
        "TRACK": [1, "CLICK", 0, 1.0, 0.0, 0, 0, 2, 3, 5, 0, 0, 0]
    })
    result = await get_track_meter(mock_client, 1)
    assert abs(result["peak_l"]) < 0.01
    assert abs(result["peak_r"]) < 0.01


@pytest.mark.asyncio
async def test_get_track_meter_not_found(mock_client):
    """Non-existent track should raise ValueError."""
    mock_client.send_command = AsyncMock(return_value={})
    with pytest.raises(ValueError, match="not found"):
        await get_track_meter(mock_client, 99)


# --- db10_to_linear tests (dB*10 values) ---


def test_db10_to_linear_silence():
    """Floor value -1500 should be 0.0."""
    assert db10_to_linear(-1500.0) == 0.0
    assert db10_to_linear(-2000.0) == 0.0


def test_db10_to_linear_unity():
    """0 dB*10 = 0 dB = linear 1.0."""
    assert abs(db10_to_linear(0.0) - 1.0) < 0.001


def test_db10_to_linear_minus_6db():
    """-60 dB*10 = -6 dB ≈ linear 0.501."""
    assert abs(db10_to_linear(-60.0) - 0.501) < 0.01


def test_cb_to_linear_alias():
    """cb_to_linear is an alias for db10_to_linear (backwards compat)."""
    assert cb_to_linear(-60.0) == db10_to_linear(-60.0)
