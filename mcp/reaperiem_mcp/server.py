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
