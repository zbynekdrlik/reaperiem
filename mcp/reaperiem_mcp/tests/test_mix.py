"""Tests for mix tools (send level and pan control)."""

import pytest
import pytest_asyncio
from unittest.mock import AsyncMock, patch

from reaperiem_mcp.lib.reaper_http import ReaperHTTPClient
from reaperiem_mcp.tools.mix import set_send_pan


@pytest.fixture
def mock_client():
    """Create a mock ReaperHTTPClient."""
    client = ReaperHTTPClient(host="iem.lan", port=8080)
    client.send_command = AsyncMock(return_value={})
    return client


@pytest.mark.asyncio
async def test_set_send_pan_center(mock_client):
    """Pan 0.0 (center) should map to REAPER value 0.5."""
    result = await set_send_pan(mock_client, 1, 0, 0.0)
    mock_client.send_command.assert_called_once_with(
        "SET/TRACK/1/SEND/0/PAN/0.5"
    )
    assert "center" in result.lower() or "0.0" in result


@pytest.mark.asyncio
async def test_set_send_pan_hard_left(mock_client):
    """Pan -1.0 (hard left) should map to REAPER value 0.0."""
    result = await set_send_pan(mock_client, 1, 0, -1.0)
    mock_client.send_command.assert_called_once_with(
        "SET/TRACK/1/SEND/0/PAN/0.0"
    )
    assert "left" in result.lower() or "-1.0" in result


@pytest.mark.asyncio
async def test_set_send_pan_hard_right(mock_client):
    """Pan 1.0 (hard right) should map to REAPER value 1.0."""
    result = await set_send_pan(mock_client, 1, 0, 1.0)
    mock_client.send_command.assert_called_once_with(
        "SET/TRACK/1/SEND/0/PAN/1.0"
    )
    assert "right" in result.lower() or "1.0" in result


@pytest.mark.asyncio
async def test_set_send_pan_partial(mock_client):
    """Pan 0.5 should map to REAPER value 0.75."""
    result = await set_send_pan(mock_client, 3, 2, 0.5)
    mock_client.send_command.assert_called_once_with(
        "SET/TRACK/3/SEND/2/PAN/0.75"
    )
    assert "0.5" in result


@pytest.mark.asyncio
async def test_set_send_pan_negative_partial(mock_client):
    """Pan -0.5 should map to REAPER value 0.25."""
    result = await set_send_pan(mock_client, 2, 1, -0.5)
    mock_client.send_command.assert_called_once_with(
        "SET/TRACK/2/SEND/1/PAN/0.25"
    )


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
    result = await set_send_pan(mock_client, 5, 3, 0.0)
    assert "track 5" in result.lower()
    assert "send 3" in result.lower()
