#!/usr/bin/env python3
"""
check_defensive_coverage.py — enforce line coverage threshold for defensive modules.

Usage:
    python3 check_defensive_coverage.py <lcov_path> <threshold_pct>

Parses an LCOV file and computes aggregate line coverage for the backup_*/
snapshot_*/poller modules.  Exits non-zero if coverage is below the threshold.

Called from CI (mutation-test job) after `cargo llvm-cov nextest --lcov`.
"""

import re
import sys


# Source files that form the "defensive hardening" module set.
# This regex matches the end of the SF: path in LCOV output.
DEFENSIVE_PATTERN = re.compile(
    r"(backup_capture|backup_daemon|backup_restore|"
    r"backup_routes|backup_store|"
    r"snapshot_routes|snapshot_store|poller)\.rs$"
)


def parse_lcov(lcov_path: str) -> tuple[int, int, list[str]]:
    """Return (hit_lines, total_lines, files_seen) for defensive modules."""
    in_target = False
    total = 0
    hit = 0
    files_seen: list[str] = []

    with open(lcov_path) as fh:
        for raw in fh:
            line = raw.rstrip()
            if line.startswith("SF:"):
                in_target = bool(DEFENSIVE_PATTERN.search(line))
                if in_target:
                    files_seen.append(line[3:])
            elif in_target and line.startswith("DA:"):
                # Format: DA:<line_number>,<execution_count>[,<checksum>]
                parts = line[3:].split(",")
                if len(parts) >= 2:
                    total += 1
                    if int(parts[1]) > 0:
                        hit += 1

    return hit, total, files_seen


def main() -> int:
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <lcov_path> <threshold_pct>")
        return 2

    lcov_path = sys.argv[1]
    try:
        threshold = float(sys.argv[2])
    except ValueError:
        print(f"ERROR: threshold must be a number, got {sys.argv[2]!r}")
        return 2

    hit, total, files_seen = parse_lcov(lcov_path)

    if total == 0:
        print(
            "ERROR: No coverage data found for defensive modules — "
            "check feature flags or LCOV path"
        )
        return 1

    pct = hit / total * 100
    print(f"Defensive modules coverage: {hit}/{total} lines = {pct:.1f}%")
    print(f"Files measured ({len(files_seen)}):")
    for f in files_seen:
        print(f"  {f}")

    if pct < threshold:
        print(f"FAIL: {pct:.1f}% < {threshold}% threshold")
        return 1

    print(f"PASS: {pct:.1f}% >= {threshold}% threshold")
    return 0


if __name__ == "__main__":
    sys.exit(main())
