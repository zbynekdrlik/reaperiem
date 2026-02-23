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
