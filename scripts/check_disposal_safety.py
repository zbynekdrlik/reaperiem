#!/usr/bin/env python3
"""CI gate: forbid unsafe Leptos signal writes inside fire-and-forget closures.

Motivation (#153): Leptos 0.7 panics with "tried to access a reactive value
that has already been disposed" when `.set()` / `.update()` (or their
untracked variants) is called on a signal whose owning scope has been
disposed. Two common triggers:

1. A `spawn_local` task that awaits something and then writes a signal —
   the component can unmount while the task is suspended, so the resumed
   task writes to a disposed signal.

2. A `Closure::wrap(Box::new(move |...| { ... }))` that gets handed to a
   JS-side API (WebSocket.set_onmessage, setInterval, setTimeout,
   set_onclick on a raw Element, addEventListener, etc.). These closures
   live on the JS heap until their trampoline is invalidated or their
   handler is replaced; they can fire AFTER the component's reactive scope
   has already dropped, panicking when they write a signal.

This check scans every .rs file in iem-mixer/iem-ui/src/ and flags any line
inside either of those danger zones that writes to a Leptos signal via
`.set(`, `.update(`, `.set_untracked(`, or `.update_untracked(` without the
`try_` prefix.

Signals are identified by the convention used in Leptos's `signal()` return:
the writer is named `set_<something>`. The regex is scoped tightly so it
does NOT match web_sys method calls like `window.set_interval(...)`,
`opts.set_body(...)`, or `socket.set_onclose(...)`.

Allowed alternatives:

    let _ = set_x.try_set(value);
    let _ = set_x.try_update(|v| *v = value);
    let _ = set_x.try_update(|v| *v += 1);
    let _ = set_x.try_set_untracked(value);
    let _ = set_x.try_update_untracked(|v| *v = value);

Escape hatch: append `// disposal-safe: <reason>` to the flagged line and the
check will ignore it. Use this only when you can prove the write cannot race
with dispose (e.g. the closure is attached to an event source that is
provably torn down by the same scope that owns the signal).

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
# calling `.set(`, `.update(`, `.set_untracked(`, or `.update_untracked(`
# as a method. The `(?:_untracked)?` group covers both tracked and
# untracked variants; word boundaries ensure we do NOT match inside
# `try_set` / `try_update` / `try_set_untracked` / `try_update_untracked`,
# because the underscore in `try_` is a word character and breaks the
# boundary.
UNSAFE_WRITE = re.compile(
    r"\bset_\w+\s*\.\s*\b(?:set|update)(?:_untracked)?\b\s*\("
)

# Match the opening of a danger zone: either
#   spawn_local(async { ...
#   spawn_local(async move { ...
#   Closure::wrap(Box::new(move || { ...
#   Closure::wrap(Box::new(move |_args_| { ...
#
# Both kinds share a common property: the brace opened on this line
# encloses code that may run AFTER the surrounding reactive scope has
# been disposed. We use a single stack to track depth regardless of the
# kind of danger zone.
DANGER_ZONE_START = re.compile(
    r"\bspawn_local\s*\(\s*async(?:\s+move)?\s*\{"
    r"|"
    r"\bClosure::wrap\s*\(\s*Box::new\s*\(\s*move\s*\|[^|]*\|\s*\{"
)

ESCAPE_HATCH = "// disposal-safe:"

# Rustfmt sometimes splits long `set_x.set(y)` statements across two lines:
#     set_alert_data
#         .set(Some(x));
# MULTILINE_WRITER_TAIL matches the first line (ends with the writer name,
# no method call yet), and MULTILINE_METHOD_HEAD matches the continuation
# on the next line. Both must match (and neither half be individually
# flagged by UNSAFE_WRITE) for the scanner to consider this a violation.
MULTILINE_WRITER_TAIL = re.compile(r"\bset_\w+\s*$")
MULTILINE_METHOD_HEAD = re.compile(
    r"^\.\s*\b(?:set|update)(?:_untracked)?\b\s*\("
)


def scan_file(path: pathlib.Path) -> list[tuple[int, str]]:
    """Return (line_number, line_text) for every violation in `path`."""
    violations: list[tuple[int, str]] = []
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    # Brace depth tracking for "inside danger zone" state.
    # When we enter a danger zone (spawn_local or Closure::wrap), we
    # record how deep we are (relative to the surrounding scope) so we
    # can tell when we leave. The stack supports nested zones.
    danger_depth_stack: list[int] = []
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

        # Detect entering a danger zone. A single line may open more than
        # one zone (e.g. a Closure::wrap inside a spawn_local), so count
        # all matches.
        for _ in DANGER_ZONE_START.finditer(stripped_code):
            danger_depth_stack.append(current_depth)

        current_depth += open_b
        current_depth -= close_b

        # Pop any danger zone frames whose block closed on this line.
        while danger_depth_stack and current_depth <= danger_depth_stack[-1]:
            danger_depth_stack.pop()

        # Inside a danger zone → flag unsafe writes.
        if danger_depth_stack:
            if UNSAFE_WRITE.search(stripped_code) and ESCAPE_HATCH not in line:
                violations.append((lineno, line.rstrip()))
            # Handle rustfmt-split writes: a line ending in `set_\w+` followed
            # by a line that starts with `.set(` / `.update(` etc. The scanner
            # otherwise misses these because both halves match no pattern on
            # their own. We check the *current* line's end + the *next* line's
            # start and flag it at the current line number so fixers land on
            # the writer side.
            if MULTILINE_WRITER_TAIL.search(stripped_code) and lineno < len(lines):
                next_line = lines[lineno]  # lines is 0-indexed, lineno is 1-indexed
                next_code = _strip_strings(next_line).lstrip()
                if (
                    MULTILINE_METHOD_HEAD.match(next_code)
                    and ESCAPE_HATCH not in line
                    and ESCAPE_HATCH not in next_line
                ):
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
