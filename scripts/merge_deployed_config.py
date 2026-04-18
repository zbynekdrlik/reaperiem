"""Merge source config.production.yaml into the deployed config.yaml.

Why this exists: the deployed config file on iem.lan contains auto-generated
secrets (jwt_secret, vapid_private_key) that must survive CI deploys, but
changes to `inputs`, `members`, and `dante_outputs` in the source file
SHOULD propagate on every deploy so adding a new input track or member
doesn't require manual hand-editing of the deployed config.

Behaviour:
  - If DEST does not exist: copy SRC verbatim.
  - If DEST exists: load both, overwrite only the keys in OVERWRITE_KEYS
    on DEST with values from SRC, write DEST back.
  - `pins` is preserved (it's operational state, not a code-level constant).
  - `jwt_secret`, `vapid_private_key`, `tls`, `tls_cert`, `tls_key`,
    `https_domain`, `reaper_url`, `port`, `https_port` are preserved.

Usage:
  python merge_deployed_config.py <src.yaml> <dest.yaml>

Exit codes:
  0 on success, 1 on any error.
"""

import os
import sys
from pathlib import Path

import yaml

# Keys that are code-level configuration — source of truth lives in the
# repo's config.production.yaml, so they must be refreshed on every deploy.
OVERWRITE_KEYS = ("inputs", "members", "dante_outputs")


def merge(src_path: Path, dest_path: Path) -> str:
    """Return a human-readable summary of what happened."""
    with src_path.open("r", encoding="utf-8") as f:
        src = yaml.safe_load(f)

    if not isinstance(src, dict):
        raise ValueError(f"source {src_path} did not parse as a YAML mapping")

    if not dest_path.exists():
        # Fresh install — deploy the whole source verbatim.
        dest_path.write_bytes(src_path.read_bytes())
        return f"Fresh deploy: copied {src_path} -> {dest_path}"

    with dest_path.open("r", encoding="utf-8") as f:
        dest = yaml.safe_load(f) or {}

    if not isinstance(dest, dict):
        raise ValueError(
            f"destination {dest_path} exists but did not parse as a YAML mapping"
        )

    # Overwrite only the listed keys; leave all others (incl. secrets) intact.
    updated = []
    for key in OVERWRITE_KEYS:
        if key not in src:
            # Source has no such key — leave dest untouched.
            continue
        before = dest.get(key)
        after = src[key]
        if before != after:
            dest[key] = after
            updated.append(key)

    if not updated:
        return "No changes: inputs/members/dante_outputs already match source"

    # Atomic write: dump to a sibling .tmp file and os.replace() onto the
    # destination. If the process is killed mid-write (CI cancel, runner
    # reboot, disk full), the destination is either the old valid file or
    # the new valid file — never a truncated intermediate state. This
    # matters because dest_path holds auto-generated secrets (jwt_secret,
    # vapid_private_key) that are NOT in the source and would be lost
    # irrecoverably if the file were corrupted.
    tmp_path = dest_path.with_suffix(dest_path.suffix + ".tmp")
    with tmp_path.open("w", encoding="utf-8") as f:
        yaml.safe_dump(
            dest,
            f,
            sort_keys=False,  # keep field order stable / readable
            default_flow_style=False,
            allow_unicode=True,
        )
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp_path, dest_path)
    return f"Merged: refreshed {', '.join(updated)} (secrets preserved)"


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: merge_deployed_config.py <src.yaml> <dest.yaml>", file=sys.stderr)
        return 1
    src = Path(sys.argv[1])
    dest = Path(sys.argv[2])
    if not src.exists():
        print(f"ERROR: source file not found: {src}", file=sys.stderr)
        return 1
    print(merge(src, dest))
    return 0


if __name__ == "__main__":
    sys.exit(main())
