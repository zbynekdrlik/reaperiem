#!/usr/bin/env python3
"""CI gate: forbid unsafe Leptos signal writes anywhere in iem-ui sources.

Motivation (#153): Leptos 0.7 panics with "tried to access a reactive value
that has already been disposed" when `.set()` / `.update()` (or their
untracked variants) is called on a signal whose owning scope has been
disposed. The original trigger was a `spawn_local` task or a
`Closure::wrap(...)` that outlived the component that owned the signal.

The first version of this scanner only flagged writes syntactically
inside a `spawn_local` or `Closure::wrap` block. That was too narrow:
helper functions called *from* those blocks can write a plain `.set()`,
and the scanner missed them because the write site was in the helper's
body, not inside a danger-zone closure. A production panic on the
Android PWA (url="/" after navigating back) was caused by exactly this
pattern — a helper called from a navigate-back handler wrote a signal
whose owning scope had already been disposed.

The fix is to make the rule CONTEXT-FREE. Any call of the form

    <receiver>.set(...)
    <receiver>.update(...)
    <receiver>.set_untracked(...)
    <receiver>.update_untracked(...)

where `<receiver>` matches the Leptos writer convention `set_\\w+` is a
violation, anywhere in the file. Helper functions are covered for free
because the regex fires on the write site regardless of the enclosing
scope.

This is safe because:

1. Web_sys setters like `window.set_interval_with_callback(...)`,
   `opts.set_body(...)`, `socket.set_onclose(...)`, `element.set_onclick(...)`,
   and `headers.set(...)` do NOT match — their receivers are not named
   `set_*`, so the `\\bset_\\w+\\s*\\.` prefix of the regex fails.

2. The `try_` prefixed forms (`try_set`, `try_update`, `try_set_untracked`,
   `try_update_untracked`) do NOT match — the `\\b` word boundary before
   `(?:set|update)` requires the preceding character to NOT be a word
   character, and the underscore in `try_set` IS a word character, so
   the boundary fails inside `try_set`.

3. Rustfmt-split writes like

       set_alert_data
           .set(Some(x));

   are handled by a secondary pass: when the current line ends with
   `set_\\w+` and the next line starts with `.set(` / `.update(`, we
   flag the current line so fixers land on the writer name.

Allowed alternatives:

    let _ = set_x.try_set(value);
    let _ = set_x.try_update(|v| *v = value);
    let _ = set_x.try_update(|v| *v += 1);
    let _ = set_x.try_set_untracked(value);
    let _ = set_x.try_update_untracked(|v| *v = value);

Escape hatch: append `// disposal-safe: <reason>` to the flagged line and
the check will ignore it. Use this only when you can prove the write
cannot race with dispose (e.g. the write happens during initial mount
before any async work can suspend).

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
# as a method.
#
# Why the word boundaries matter:
#   * The leading `\b` before `set_\w+` ensures we don't match inside
#     identifiers like `try_set_foo` — the character before `set` in
#     `try_set_foo` is `_`, which is a word character, so `\bset_` fails.
#   * The `\b` before `(?:set|update)` ensures we don't match
#     `.try_set(...)` / `.try_update(...)` — same reasoning: the
#     character before `set` in `try_set` is `_`.
#
# Why `\.` is escaped: we want a literal dot (method call), not "any
# character".
#
# Why web_sys methods are not matched:
#   `window.set_interval_with_callback(...)` → receiver is `window`, not
#   `set_*`, so `\bset_\w+\s*\.` fails on the receiver.
UNSAFE_WRITE = re.compile(
    r"\bset_\w+\s*\.\s*\b(?:set|update)(?:_untracked)?\b\s*\("
)

# Rustfmt-split writes:
#     set_alert_data
#         .set(Some(x));
#
# MULTILINE_WRITER_TAIL matches a line that ENDS with a writer name and
# no method call (so the method lives on the next line). MULTILINE_METHOD_HEAD
# matches a line that STARTS with `.set(` / `.update(` etc. When both fire
# on consecutive lines, we flag the writer line.
MULTILINE_WRITER_TAIL = re.compile(r"\bset_\w+\s*$")
MULTILINE_METHOD_HEAD = re.compile(
    r"^\.\s*\b(?:set|update)(?:_untracked)?\b\s*\("
)

ESCAPE_HATCH = "// disposal-safe:"


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


def scan_file(path: pathlib.Path) -> list[tuple[int, str]]:
    """Return (line_number, line_text) for every violation in `path`.

    The rule is context-free: any `set_*.set(...)`, `set_*.update(...)`,
    `set_*.set_untracked(...)`, or `set_*.update_untracked(...)` call
    is a violation anywhere in the file, unless the line has the
    `// disposal-safe: ...` escape hatch.
    """
    violations: list[tuple[int, str]] = []
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    for lineno, line in enumerate(lines, start=1):
        stripped = line.strip()

        # Ignore pure line comments so a `// foo.set(bar)` example in a
        # doc comment doesn't trigger the gate. Trailing comments like
        # `set_foo.set(1); // note` are NOT skipped — the regex still
        # fires on the code before the comment. The `// disposal-safe:`
        # escape hatch is handled separately below.
        if stripped.startswith("//"):
            continue

        # Strip inline string literals so regexes don't fire on string
        # content like `let s = "set_foo.set(1)";`.
        stripped_code = _strip_strings(line)

        # Single-line write: `set_foo.set(1)` / `set_foo.update(...)` etc.
        if UNSAFE_WRITE.search(stripped_code) and ESCAPE_HATCH not in line:
            violations.append((lineno, line.rstrip()))

        # Rustfmt-split write: current line ends in `set_\w+`, next line
        # starts with `.set(` / `.update(` etc. Flag at the current line
        # so fixers land on the writer name.
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
            "::error::Unsafe Leptos signal writes (disposal race, #153):"
        )
        for rel, lineno, line in all_violations:
            print(f"  {rel}:{lineno}: {line.strip()}")
        print()
        print("Fix: replace `.set(x)` with `.try_set(x)` or")
        print("     `.try_update(|v| *v = x)`. Use `let _ =` to discard")
        print("     the Option<...> result.")
        print()
        print("This rule is context-free: any `set_*.set(...)` is a violation")
        print("anywhere in the file, because helper functions called from")
        print("spawn_local / Closure::wrap / event handlers can race with")
        print("scope disposal and panic.")
        print()
        print("Escape hatch: append `// disposal-safe: <reason>` to the line")
        print("if you can prove the write cannot race with scope disposal.")
        return 1

    print(
        f"OK: no unsafe signal writes found "
        f"({SCAN_ROOT.relative_to(REPO_ROOT)}/**/*.rs)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
