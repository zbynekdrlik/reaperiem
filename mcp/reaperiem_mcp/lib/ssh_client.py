"""SSH client for running git commands on remote Windows machine."""

import shlex
from dataclasses import dataclass, field
from pathlib import Path

import paramiko


@dataclass
class SSHGitClient:
    """SSH client for git operations on iem.lan Windows PC."""

    host: str
    username: str
    repo_path: str
    key_path: str | None = None
    password: str | None = None
    port: int = 22
    _client: paramiko.SSHClient = field(default_factory=paramiko.SSHClient, repr=False)

    def __post_init__(self):
        """Initialize SSH client."""
        self._client.set_missing_host_key_policy(paramiko.AutoAddPolicy())

    def _build_git_command(self, git_cmd: str) -> str:
        """Build command to run git in repo directory.

        Uses PowerShell-style cd for Windows compatibility.
        """
        # Escape backslashes for shell
        escaped_path = self.repo_path.replace("\\", "\\\\")
        return f'cd "{escaped_path}" && git {git_cmd}'

    def _connect(self) -> None:
        """Establish SSH connection."""
        connect_kwargs = {
            "hostname": self.host,
            "port": self.port,
            "username": self.username,
        }

        if self.key_path:
            key_file = Path(self.key_path).expanduser()
            connect_kwargs["key_filename"] = str(key_file)
        elif self.password:
            connect_kwargs["password"] = self.password

        self._client.connect(**connect_kwargs)

    def _disconnect(self) -> None:
        """Close SSH connection."""
        self._client.close()

    def run_git(self, git_cmd: str) -> tuple[str, str, int]:
        """Run git command on remote and return (stdout, stderr, exit_code)."""
        cmd = self._build_git_command(git_cmd)
        try:
            self._connect()
            stdin, stdout, stderr = self._client.exec_command(cmd)
            exit_code = stdout.channel.recv_exit_status()
            return (
                stdout.read().decode("utf-8"),
                stderr.read().decode("utf-8"),
                exit_code,
            )
        finally:
            self._disconnect()

    def status(self) -> str:
        """Get git status."""
        stdout, stderr, code = self.run_git("status --porcelain")
        if code != 0:
            raise RuntimeError(f"git status failed: {stderr}")
        return stdout

    def add(self, paths: str = ".") -> None:
        """Stage files."""
        stdout, stderr, code = self.run_git(f"add -- {shlex.quote(paths)}")
        if code != 0:
            raise RuntimeError(f"git add failed: {stderr}")

    def commit(self, message: str) -> str:
        """Create commit with message."""
        escaped_msg = shlex.quote(message)
        stdout, stderr, code = self.run_git(f"commit -m {escaped_msg}")
        if code != 0:
            if "nothing to commit" in stdout or "nothing to commit" in stderr:
                return "Nothing to commit"
            raise RuntimeError(f"git commit failed: {stderr}")
        return stdout

    def push(self) -> str:
        """Push to remote."""
        stdout, stderr, code = self.run_git("push")
        if code != 0:
            raise RuntimeError(f"git push failed: {stderr}")
        return stdout

    def log(self, count: int = 5) -> str:
        """Get recent commits."""
        count = max(1, min(count, 100))
        stdout, stderr, code = self.run_git(
            f"log --oneline -n {count}"
        )
        if code != 0:
            raise RuntimeError(f"git log failed: {stderr}")
        return stdout
