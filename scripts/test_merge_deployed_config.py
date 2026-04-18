#!/usr/bin/env python3
"""Unit tests for `scripts/merge_deployed_config.py`.

Run: python3 scripts/test_merge_deployed_config.py

The tests cover the two invariants that matter in production:

1. Secrets (jwt_secret, vapid_private_key, etc.) in the deployed config
   are never lost when OVERWRITE_KEYS (inputs/members/dante_outputs) are
   refreshed from source.
2. The write is atomic — a .tmp sibling is used and os.replace() finishes
   the transaction, so a crash mid-write leaves either the old or the new
   file, never a truncated one.
"""

from __future__ import annotations

import pathlib
import sys
import tempfile
import traceback

import yaml

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import merge_deployed_config as mdc  # noqa: E402


def _write(path: pathlib.Path, data: dict) -> None:
    path.write_text(yaml.safe_dump(data, sort_keys=False), encoding="utf-8")


def test_fresh_deploy_copies_source_verbatim() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = pathlib.Path(tmp)
        src = tmp_path / "src.yaml"
        dest = tmp_path / "dest.yaml"
        _write(src, {"inputs": [{"name": "FOO", "dante_input": 1}], "port": 80})
        # dest does not exist yet — fresh install path
        summary = mdc.merge(src, dest)
        assert dest.exists(), "dest should exist after fresh deploy"
        assert "Fresh deploy" in summary, summary
        loaded = yaml.safe_load(dest.read_text(encoding="utf-8"))
        assert loaded["port"] == 80
        assert loaded["inputs"][0]["name"] == "FOO"


def test_merge_preserves_secrets() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = pathlib.Path(tmp)
        src = tmp_path / "src.yaml"
        dest = tmp_path / "dest.yaml"
        _write(src, {"inputs": [{"name": "NEW", "dante_input": 1}]})
        _write(
            dest,
            {
                "inputs": [{"name": "OLD", "dante_input": 1}],
                "jwt_secret": "auto-keep-me",
                "vapid_private_key": "keep-this-too",
                "port": 80,
            },
        )
        mdc.merge(src, dest)
        loaded = yaml.safe_load(dest.read_text(encoding="utf-8"))
        assert loaded["inputs"][0]["name"] == "NEW", "inputs should be refreshed"
        assert loaded["jwt_secret"] == "auto-keep-me", "jwt_secret must survive"
        assert loaded["vapid_private_key"] == "keep-this-too"
        assert loaded["port"] == 80


def test_merge_noop_when_keys_match() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = pathlib.Path(tmp)
        src = tmp_path / "src.yaml"
        dest = tmp_path / "dest.yaml"
        shared = {"inputs": [{"name": "SAME", "dante_input": 1}]}
        _write(src, shared)
        _write(dest, {**shared, "jwt_secret": "x"})
        summary = mdc.merge(src, dest)
        assert "No changes" in summary, summary


def test_atomic_write_leaves_no_tmp_file() -> None:
    """After a successful merge, the .tmp sibling must not exist."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = pathlib.Path(tmp)
        src = tmp_path / "src.yaml"
        dest = tmp_path / "dest.yaml"
        _write(src, {"inputs": [{"name": "NEW", "dante_input": 1}]})
        _write(
            dest,
            {"inputs": [{"name": "OLD", "dante_input": 1}], "jwt_secret": "x"},
        )
        mdc.merge(src, dest)
        tmp_sibling = dest.with_suffix(dest.suffix + ".tmp")
        assert not tmp_sibling.exists(), (
            f"temporary file {tmp_sibling} should not exist after successful merge"
        )
        # Dest must be valid YAML
        loaded = yaml.safe_load(dest.read_text(encoding="utf-8"))
        assert loaded["jwt_secret"] == "x"


def test_crash_before_replace_leaves_original_intact(monkeypatch=None) -> None:
    """Simulate a crash after tmp is written but before os.replace(): the
    original dest must still contain the pre-merge content unchanged."""
    import os as real_os

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = pathlib.Path(tmp)
        src = tmp_path / "src.yaml"
        dest = tmp_path / "dest.yaml"
        _write(src, {"inputs": [{"name": "NEW", "dante_input": 1}]})
        original = {"inputs": [{"name": "OLD", "dante_input": 1}], "jwt_secret": "keep"}
        _write(dest, original)

        # Patch os.replace inside the mdc module to raise — simulates a
        # crash right at the atomic-rename point.
        original_replace = real_os.replace

        def boom(*_args, **_kwargs):
            raise KeyboardInterrupt("simulated crash during replace")

        mdc.os.replace = boom  # type: ignore[attr-defined]
        try:
            try:
                mdc.merge(src, dest)
            except KeyboardInterrupt:
                pass  # expected
            # Original dest must be untouched (byte-for-byte).
            loaded = yaml.safe_load(dest.read_text(encoding="utf-8"))
            assert loaded == original, (
                f"dest must be unchanged after crash; got {loaded}"
            )
        finally:
            mdc.os.replace = original_replace  # type: ignore[attr-defined]


def run_all() -> int:
    tests = [
        test_fresh_deploy_copies_source_verbatim,
        test_merge_preserves_secrets,
        test_merge_noop_when_keys_match,
        test_atomic_write_leaves_no_tmp_file,
        test_crash_before_replace_leaves_original_intact,
    ]
    failures = 0
    for t in tests:
        try:
            t()
            print(f"OK  {t.__name__}")
        except Exception:
            failures += 1
            print(f"FAIL {t.__name__}")
            traceback.print_exc()
    if failures:
        print(f"\n{failures}/{len(tests)} tests failed")
        return 1
    print(f"\nAll {len(tests)} tests passed")
    return 0


if __name__ == "__main__":
    sys.exit(run_all())
