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
