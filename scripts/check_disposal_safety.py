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

# Second-pass detection: track locally-bound RwSignal names.
#
# Leptos `RwSignal::new(x)` is commonly bound to arbitrary identifiers
# that do NOT follow the `set_*` convention (e.g.
# `let local_value = RwSignal::new(0.0)`). The primary UNSAFE_WRITE regex
# misses these because its receiver anchor is `set_\w+`. To cover them,
# we do a first pass over each file to collect the set of identifiers
# bound to `RwSignal::new(...)`, then a second pass flags any
# `<name>.set(` / `<name>.update(` call on one of those tracked names.
#
# Two binding shapes are tracked:
#   1. `let <name> = RwSignal::new(...)` (and `let mut <name> = ...`)
#   2. Struct field initializers of the form `<field>: RwSignal::new(...)`
#      inside struct literals. This is conservative — the field name is
#      added to the tracked set even though the scanner cannot tell
#      which *receiver expression* owns that field. In practice, any
#      `foo.<field>.set(...)` call on a struct with such a field is a
#      disposal-race candidate and deserves `try_set` anyway.
RWSIGNAL_BINDING = re.compile(
    r"^\s*let\s+(?:mut\s+)?(\w+)\s*=\s*RwSignal::new\s*\("
)
RWSIGNAL_FIELD_BINDING = re.compile(
    r"^\s*(\w+)\s*:\s*RwSignal::new\s*\("
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


def _collect_rwsignal_names(lines: list[str]) -> set[str]:
    """First pass: find every identifier in `lines` bound to `RwSignal::new(...)`.

    Scans both `let <name> = RwSignal::new(...)` and struct-field
    initializers `<field>: RwSignal::new(...)`. Comment-only lines are
    skipped, and string literals are stripped before matching so a
    literal like `"let x = RwSignal::new(0)"` does not pollute the set.
    """
    names: set[str] = set()
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("//"):
            continue
        code = _strip_strings(line)
        m = RWSIGNAL_BINDING.match(code)
        if m:
            names.add(m.group(1))
            continue
        mf = RWSIGNAL_FIELD_BINDING.match(code)
        if mf:
            names.add(mf.group(1))
    return names


def _build_tracked_name_regex(names: set[str]) -> re.Pattern[str] | None:
    r"""Build a single regex that flags `.set(` / `.update(` calls on
    any of the tracked identifiers.

    Returns None if `names` is empty (nothing to scan for).

    Example output for names={"local_value", "curve_trigger"}:
        \b(?:local_value|curve_trigger)\s*\.\s*\b(?:set|update)(?:_untracked)?\b\s*\(
    """
    if not names:
        return None
    alt = "|".join(sorted(re.escape(n) for n in names))
    return re.compile(
        r"\b(?:" + alt + r")\s*\.\s*\b(?:set|update)(?:_untracked)?\b\s*\("
    )


def scan_file(path: pathlib.Path) -> list[tuple[int, str]]:
    """Return (line_number, line_text) for every violation in `path`.

    The rule is context-free: any `set_*.set(...)`, `set_*.update(...)`,
    `set_*.set_untracked(...)`, or `set_*.update_untracked(...)` call
    is a violation anywhere in the file, unless the line has the
    `// disposal-safe: ...` escape hatch.

    In addition, a second-pass check flags `.set(` / `.update(` calls
    on any identifier locally bound to `RwSignal::new(...)` — this
    catches Leptos locals whose names don't follow the `set_*`
    convention (e.g. `let local_value = RwSignal::new(0.0)`).
    """
    violations: list[tuple[int, str]] = []
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    # First pass: collect every identifier in this file bound to an
    # RwSignal. Second pass will flag writes on these names.
    tracked_names = _collect_rwsignal_names(lines)
    tracked_re = _build_tracked_name_regex(tracked_names)

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
        matched_primary = False
        if UNSAFE_WRITE.search(stripped_code) and ESCAPE_HATCH not in line:
            violations.append((lineno, line.rstrip()))
            matched_primary = True

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

        # Second-pass: writes on locally-tracked RwSignal names. Only
        # fire if the primary regex didn't already match on this line,
        # to avoid double-reporting. Respects the `// disposal-safe:`
        # escape hatch just like the primary pass.
        if (
            not matched_primary
            and tracked_re is not None
            and tracked_re.search(stripped_code)
            and ESCAPE_HATCH not in line
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
