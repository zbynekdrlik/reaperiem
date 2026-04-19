#!/usr/bin/env python3
"""Forbid validating REAPER `track_index` values against `inputs.len()`.

Why this guard exists (#179):
  The REAPER track_index of an input track is NOT the same as its
  position in `config.inputs`. When an input track is added to an
  existing REAPER project at a later row position (e.g. ALEX kl at
  REAPER track 44 with only 23 inputs in the YAML), any validator
  that does `track_index <= inputs.len()` will silently reject the
  real REAPER index, drop the WS command, and leave REAPER state
  unchanged — while the UI still shows the change optimistically.

  This exact bug made the ALEX kl keyboard uncontrollable during a
  live service. Members could not mute or adjust the fader; the
  keyboard stayed pinned at unity in every IEM.

  The authoritative set of valid input track indices is returned by
  `collect_valid_input_indices(inputs, resolved)` in proxy.rs —
  based on REAPER-discovered indices, not config position count.

This scanner fails CI if it finds any of these patterns:

  1. A comparison that upper-bounds something named track_index / ti
     by a count-like variable (input_count, inputs.len()).
  2. A call to `validate_track_index(..., inputs.len())` or
     `validate_track_index(..., input_count)`.

It is intentionally narrow — it greps for exact-enough patterns to
catch the specific regression without false-positiving on unrelated
code (e.g. `for i in 0..inputs.len()` loops).
"""

from __future__ import annotations

import pathlib
import re
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
SEARCH_DIRS = [REPO_ROOT / "iem-mixer" / "crates" / "iem-server" / "src"]

# Patterns forbidden inside Rust source. Ordered from most specific to
# most general so the error message points at the real problem.
_COUNT_RE = r"(?:\w+\.)?inputs\.len\(\)|input_count"
_TI_RE = r"track_index|ti"

FORBIDDEN_PATTERNS = [
    (
        re.compile(
            r"validate_track_index\s*\(\s*[^,]+,\s*(?:" + _COUNT_RE + r")\s*\)"
        ),
        "validate_track_index() must take a HashSet of valid REAPER indices, "
        "not `inputs.len()` / `input_count`. Use collect_valid_input_indices().",
    ),
    (
        re.compile(
            r"\b(?:" + _TI_RE + r")\s*[<>]=?\s*(?:" + _COUNT_RE + r")"
        ),
        "Comparing track_index / ti against inputs.len() or input_count is the "
        "#179 regression — REAPER track indices can exceed input count. "
        "Use `collect_valid_input_indices(..).contains(&ti)` instead.",
    ),
    (
        re.compile(
            r"\b(?:" + _COUNT_RE + r")\s*[<>]=?\s*(?:" + _TI_RE + r")\b"
        ),
        "Inverse form of the #179 regression — same fix: validate against "
        "`collect_valid_input_indices(..)`.",
    ),
]

# File names whose content is definitional for the forbidden pattern —
# skip them so the docstring / doc-comment mentioning the rule doesn't
# trigger the rule.
EXEMPT_FILES = {
    # proxy.rs is allowed to mention the pattern ONLY in comments/docs
    # describing why it's wrong. The scanner ignores comment-only hits.
}


def is_comment_line(line: str) -> bool:
    """Return True if the stripped line starts with // or is inside a
    Rust doc-comment block (/// or //!)."""
    s = line.lstrip()
    return s.startswith("//") or s.startswith("///") or s.startswith("//!")


def is_test_regression_assertion(line: str) -> bool:
    """Some tests legitimately name the forbidden pattern in assertion
    strings ('A validator that used ti <= inputs.len() ...'). These
    are in #[test] contexts and are string literals — they don't
    execute the pattern, they document why it's banned. Detect by
    presence of quotes + comment-like framing."""
    # If the match is inside a string literal, ignore.
    # Heuristic: line contains a double-quoted string that contains the
    # forbidden text and nothing else of concern.
    return line.count('"') >= 2 and (
        "regression" in line.lower()
        or "would accept" in line.lower()
        or "would reject" in line.lower()
        or "silently rejected" in line.lower()
    )


def scan_file(path: pathlib.Path) -> list[tuple[int, str, str]]:
    violations: list[tuple[int, str, str]] = []
    try:
        text = path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        return violations
    for lineno, line in enumerate(text.splitlines(), start=1):
        if is_comment_line(line):
            continue
        if is_test_regression_assertion(line):
            continue
        for regex, why in FORBIDDEN_PATTERNS:
            if regex.search(line):
                violations.append((lineno, line.rstrip(), why))
                break
    return violations


def main() -> int:
    total = 0
    for root in SEARCH_DIRS:
        for rs_file in sorted(root.rglob("*.rs")):
            rel = rs_file.relative_to(REPO_ROOT)
            if str(rel) in EXEMPT_FILES:
                continue
            hits = scan_file(rs_file)
            for lineno, line, why in hits:
                print(f"::error file={rel},line={lineno}::#179 regression — {why}")
                print(f"  {rel}:{lineno}: {line}")
                total += 1
    if total == 0:
        print(
            "OK: no track_index <= inputs.len() / input_count validators "
            "found — #179 regression protection is intact."
        )
        return 0
    print(f"\nFAIL: {total} #179-style validator(s) found. See messages above.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
