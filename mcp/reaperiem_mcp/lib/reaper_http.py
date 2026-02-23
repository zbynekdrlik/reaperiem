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

    async def set_extstate(self, section: str, key: str, value: str) -> None:
        """Set ExtState value that ReaScripts can read."""
        await self.send_command(f"SET/EXTSTATE/{section}/{key}/{value}")

    async def get_extstate(self, section: str, key: str) -> str | None:
        """Get ExtState value."""
        result = await self.send_command(f"EXTSTATE/{section}/{key}")
        return result.get("EXTSTATE")

    async def trigger_action(self, action_id: str | int) -> None:
        """Trigger a REAPER action by command ID.

        action_id can be:
        - Integer command ID (e.g., 1007 for play)
        - String command ID for registered scripts (e.g., "_RSxxxx...")
        """
        await self.send_command(str(action_id))
