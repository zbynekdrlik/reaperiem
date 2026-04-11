#!/usr/bin/env python3
"""CI gate: forbid unsafe signal writes inside spawn_local async blocks.

Motivation (#153): Leptos 0.7 panics with "tried to access a reactive value
that has already been disposed" when `.set()` / `.update()` is called on a
signal whose owning scope has been disposed. The most common trigger is a
`spawn_local` task that awaits something and then writes a signal — the
component can unmount while the task is suspended, so the resumed task
writes to a disposed signal.

This check scans every .rs file in iem-mixer/iem-ui/src/ and flags any line
inside a `spawn_local(async move { ... })` or `spawn_local(async { ... })`
block that writes to a Leptos signal via `.set(` or `.update(` without the
`try_` prefix.

Signals are identified by the convention used in Leptos's `signal()` return:
the writer is named `set_<something>`. The regex is scoped tightly so it
does NOT match web_sys method calls like `window.set_interval(...)`,
`opts.set_body(...)`, or `socket.set_onclose(...)`.

Allowed alternatives inside spawn_local:

    let _ = set_x.try_set(value);
    let _ = set_x.try_update(|v| *v = value);
    let _ = set_x.try_update(|v| *v += 1);

Escape hatch: append `// disposal-safe: <reason>` to the flagged line and the
check will ignore it. Use this only when you can prove the write cannot race
with dispose (e.g. immediately after a sync check that the owner is alive).

Usage:
    python3 scripts/check_disposal_safety.py

Exits 0 if clean, 1 if any violation is found.
"""

from __future__ import annotations

import pathlib
import re
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
SCAN_ROOT = REPO_ROOT / "iem-mixer" / "iem-ui" / "src"

# Match identifiers that follow the Leptos writer convention (`set_*`)
# calling `.set(` or `.update(` as a method. Word boundaries ensure we do
# NOT match inside `try_set` / `try_update`, because the underscore in
# `try_` is a word character and breaks the boundary.
UNSAFE_WRITE = re.compile(
    r"\bset_\w+\s*\.\s*\b(set|update)\b\s*\("
)

# Match spawn_local(async { or spawn_local(async move {
SPAWN_LOCAL_START = re.compile(
    r"\bspawn_local\s*\(\s*async(?:\s+move)?\s*\{"
)

ESCAPE_HATCH = "// disposal-safe:"


def scan_file(path: pathlib.Path) -> list[tuple[int, str]]:
    """Return (line_number, line_text) for every violation in `path`."""
    violations: list[tuple[int, str]] = []
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    # Brace depth tracking for "inside spawn_local" state.
    # When we enter a spawn_local block, we record how deep we are
    # (relative to the surrounding scope) so we can tell when we leave.
    spawn_depth_stack: list[int] = []
    current_depth = 0

    for lineno, line in enumerate(lines, start=1):
        stripped = line.strip()

        # Ignore pure line comments so a `// foo.set(bar)` in a doc comment
        # doesn't trigger the gate.
        if stripped.startswith("//"):
            # still need to update brace depth from the rest of the file,
            # but comments rarely contain braces that matter — skip.
            continue

        # Strip inline string literals (very naive: remove "..." spans) so
        # regexes don't fire on string content.
        stripped_code = _strip_strings(line)

        # Count braces AFTER stripping strings so a `"foo{bar}"` literal
        # doesn't confuse the depth tracker.
        open_b = stripped_code.count("{")
        close_b = stripped_code.count("}")

        # Detect entering a spawn_local block.
        # The brace opened by the spawn_local itself counts toward depth.
        if SPAWN_LOCAL_START.search(stripped_code):
            spawn_depth_stack.append(current_depth)

        current_depth += open_b
        current_depth -= close_b

        # Pop any spawn_local frames whose block closed on this line.
        while spawn_depth_stack and current_depth <= spawn_depth_stack[-1]:
            spawn_depth_stack.pop()

        # Inside a spawn_local block → flag unsafe writes.
        if spawn_depth_stack:
            if UNSAFE_WRITE.search(stripped_code) and ESCAPE_HATCH not in line:
                violations.append((lineno, line.rstrip()))

    return violations


def _strip_strings(line: str) -> str:
    """Remove double-quoted string contents from a line.

    Very naive — does not handle escaped quotes, raw strings, or multi-line
    strings. Good enough for scanning single-line .rs source where string
    literals that would trigger the regex are rare. False negatives are
    acceptable; the goal is to avoid false positives on string content.
    """
    result = []
    in_string = False
    for ch in line:
        if ch == '"':
            in_string = not in_string
            result.append('"')
        elif in_string:
            result.append(" ")
        else:
            result.append(ch)
    return "".join(result)


def main() -> int:
    if not SCAN_ROOT.is_dir():
        print(f"::error::Scan root does not exist: {SCAN_ROOT}", file=sys.stderr)
        return 2

    all_violations: list[tuple[pathlib.Path, int, str]] = []
    for rs_path in sorted(SCAN_ROOT.rglob("*.rs")):
        for lineno, line in scan_file(rs_path):
            rel = rs_path.relative_to(REPO_ROOT)
            all_violations.append((rel, lineno, line))

    if all_violations:
        print(
            "::error::Unsafe signal writes inside spawn_local blocks "
            "(Leptos disposal race, #153):"
        )
        for rel, lineno, line in all_violations:
            print(f"  {rel}:{lineno}: {line.strip()}")
        print()
        print("Fix: replace `.set(x)` with `.try_set(x)` or")
        print("     `.try_update(|v| *v = x)`. Use `let _ =` to discard")
        print("     the Option<...> result.")
        print()
        print("Escape hatch: append `// disposal-safe: <reason>` to the line")
        print("if you can prove the write cannot race with scope disposal.")
        return 1

    print(
        f"OK: no unsafe signal writes found inside spawn_local blocks "
        f"({SCAN_ROOT.relative_to(REPO_ROOT)}/**/*.rs)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
