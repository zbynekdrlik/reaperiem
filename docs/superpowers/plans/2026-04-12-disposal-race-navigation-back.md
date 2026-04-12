# Deeply Fix Navigation-Back Disposal Race — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the "tried to access a reactive value that has already been disposed" panic seen on Android PWA when navigating back from member mixer to landing, by converting every plain Leptos signal write in `iem-mixer/iem-ui/src/` to its `try_` variant and tightening the CI scanner into a context-free rule that forbids plain `.set()` / `.update()` on any `set_*` identifier.

**Architecture:** Project-wide defensive sweep (approach B from the spec). Keeps the existing component structure, replaces `.set(` / `.update(` with `.try_set(` / `.try_update(` discarded via `let _ =`, and rewrites `scripts/check_disposal_safety.py` into a simpler context-free gate. An architectural restructuring (approach C) is tracked separately in issue #165 and is NOT part of this plan.

**Tech Stack:** Rust 2024, Leptos 0.7 WASM, Python 3 (CI scanner + self-tests), Playwright TypeScript (E2E), GitHub Actions (CI + self-hosted runner on iem.lan).

**Spec:** `docs/superpowers/specs/2026-04-12-disposal-race-navigation-back-design.md`

**Issue tracking approach C:** [#165](https://github.com/zbynekdrlik/reaperiem/issues/165)

---

## File map

### Code changes

- Modify: `iem-mixer/crates/iem-core/Cargo.toml` — version bump 1.143.0 → 1.144.0
- Modify: `iem-mixer/Cargo.toml` — version bump 1.143.0 → 1.144.0
- Modify: `iem-mixer/crates/iem-server/Cargo.toml` — version bump 1.143.0 → 1.144.0
- Modify: `iem-mixer/iem-ui/Cargo.toml` — version bump 1.143.0 → 1.144.0
- Modify: `iem-mixer/src-tauri/Cargo.toml` — version bump 1.143.0 → 1.144.0
- Modify: `iem-mixer/src-tauri/tauri.conf.json` — version bump 1.143.0 → 1.144.0
- Modify: `scripts/check_disposal_safety.py` — rewrite as context-free gate
- Modify: `scripts/test_check_disposal_safety.py` — rewrite self-tests for new rule
- Modify: 14 Rust files in `iem-mixer/iem-ui/src/` (project-wide sweep, ~255 call sites):
  - `pages/mixer.rs` (114 sites)
  - `pages/login.rs` (11 sites)
  - `pages/landing.rs` (2 sites)
  - `components/pin_change_modal.rs` (28 sites)
  - `components/fader.rs` (22 sites)
  - `components/pan.rs` (22 sites)
  - `components/audio_player.rs` (12 sites)
  - `components/eq_modal.rs` (12 sites)
  - `components/backup_section.rs` (8 sites)
  - `components/limiter_modal.rs` (8 sites)
  - `components/preset_modal.rs` (6 sites)
  - `components/snapshot_modal.rs` (5 sites)
  - `components/settings_modal.rs` (3 sites)
  - `components/talk_button.rs` (2 sites)

### New files

- Create: `iem-mixer/e2e/tests/live/navigation-back-disposal.spec.ts`
- Create: `/tmp/sweep_try_set.py` (one-shot sweep tool, NOT committed)

### Documentation

- Modify: `README.md` — v1.144.0 changelog entry

---

## Task dependency graph

```
Task 1 (version bump)                           ─┐
                                                  │
Task 2 (scanner self-tests — TDD red)  ─────────▶ │
Task 3 (scanner rewrite — TDD green)   ─────────▶ │
                                                  │
Task 4 (project-wide try_set sweep)    ─────────▶ │
Task 5 (scanner gate passes against    ─────────▶ │
         swept tree)                              │
                                                  │
Task 6 (new E2E test for navigation-back) ────── ▶│
Task 7 (changelog entry)                 ─────── ▶│
                                                  │
                                                  ▼
Task 8 (local fmt check + push + monitor CI)
                                                  │
                                                  ▼
Task 9 (post-deploy live verification via
        win-iem-snv MCP log grep)
                                                  │
                                                  ▼
Task 10 (create PR, wait for explicit merge approval)
```

Tasks 2 and 3 must run in order (red-green TDD). Task 4 depends on Task 3 (the sweep tool reuses the scanner's regex). Task 5 is a verification step after Tasks 3 and 4 both land. Tasks 6 and 7 are independent of Tasks 2-5. Task 8 must wait for everything else.

---

## Task 1: Version bump 1.143.0 → 1.144.0

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

- [ ] **Step 1: Verify current state**

Run:
```bash
grep -l '1.143.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
```
Expected: all six file paths print.

- [ ] **Step 2: Bump all six files**

Run:
```bash
sed -i 's/version = "1.143.0"/version = "1.144.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.143.0"/"version": "1.144.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 3: Verify**

Run:
```bash
grep -c '1.144.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
```
Expected: both return 1.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
        iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
        iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.144.0"
```

---

## Task 2: Rewrite scanner self-tests (TDD red)

**Files:**
- Modify: `scripts/test_check_disposal_safety.py`

The existing self-tests have "inside/outside danger zone" pairs. The new rule is context-free, so those pairs collapse into single "is it `try_` or not" cases. We write the new self-test suite FIRST, then run it against the OLD scanner and watch it fail (red), then in Task 3 rewrite the scanner to pass them (green).

- [ ] **Step 1: Replace the self-test file with the new rule set**

Overwrite `scripts/test_check_disposal_safety.py` with the following. The new suite has 16 tests. Every plain `.set()` / `.update()` on a `set_*` identifier is a violation regardless of context. Every `.try_*` is a pass. Escape hatch, comments, string literals, and web_sys setters stay in their old buckets.

```python
#!/usr/bin/env python3
"""Unit tests for `scripts/check_disposal_safety.py` (context-free rule).

Run: python3 scripts/test_check_disposal_safety.py

After the disposal-race hardening plan (2026-04-12), the scanner no longer
tracks danger zones. The rule is simple: any `.set()` / `.update()` /
`.set_untracked()` / `.update_untracked()` call on an identifier that
matches `set_\\w+` is a violation, anywhere in the file. `try_set` /
`try_update` / `try_set_untracked` / `try_update_untracked` are the only
safe variants. Web_sys methods (`window.set_interval_with_callback`,
`opts.set_body`, `headers.set`, etc.) must not match because the receiver
is not a `set_*` identifier.
"""

from __future__ import annotations

import pathlib
import sys
import tempfile
import traceback

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import check_disposal_safety as cds  # noqa: E402


def scan_src(src: str) -> list[tuple[int, str]]:
    with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False) as f:
        f.write(src)
        path = pathlib.Path(f.name)
    try:
        return cds.scan_file(path)
    finally:
        path.unlink(missing_ok=True)


_failures: list[tuple[str, str]] = []
_passed = 0


def check(name: str, cond: bool, detail: str = "") -> None:
    global _passed
    if cond:
        _passed += 1
        print(f"  ok   {name}")
    else:
        _failures.append((name, detail))
        print(f"  FAIL {name}{(' — ' + detail) if detail else ''}")


def assert_violation_at(name: str, src: str, expected_line: int) -> None:
    vs = scan_src(src)
    lines = [ln for ln, _ in vs]
    check(name, expected_line in lines, f"expected line {expected_line} in {lines}")


def assert_no_violations(name: str, src: str) -> None:
    vs = scan_src(src)
    check(name, not vs, f"unexpected violations: {vs}")


# ---------------------------------------------------------------------------
# True positives — every plain signal write is flagged, regardless of context
# ---------------------------------------------------------------------------


def test_plain_set_at_top_level_fn_is_flagged() -> None:
    src = """
fn f() {
    set_foo.set(1);
}
"""
    assert_violation_at("plain set in top-level fn is flagged", src, 3)


def test_plain_update_at_top_level_fn_is_flagged() -> None:
    src = """
fn f() {
    set_foo.update(|v| *v = 1);
}
"""
    assert_violation_at("plain update in top-level fn is flagged", src, 3)


def test_plain_set_untracked_is_flagged() -> None:
    src = """
fn f() {
    set_foo.set_untracked(1);
}
"""
    assert_violation_at("plain set_untracked is flagged", src, 3)


def test_plain_update_untracked_is_flagged() -> None:
    src = """
fn f() {
    set_foo.update_untracked(|v| *v = 1);
}
"""
    assert_violation_at("plain update_untracked is flagged", src, 3)


def test_plain_set_inside_spawn_local_is_flagged() -> None:
    src = """
fn f() {
    spawn_local(async move {
        set_foo.set(1);
    });
}
"""
    assert_violation_at("plain set inside spawn_local is flagged", src, 4)


def test_plain_set_inside_closure_wrap_is_flagged() -> None:
    src = """
fn f() {
    let cb = Closure::wrap(Box::new(move |_: Event| {
        set_state.set(Idle);
    }) as Box<dyn FnMut(_)>);
}
"""
    assert_violation_at("plain set inside Closure::wrap is flagged", src, 4)


def test_plain_set_inside_on_click_is_flagged() -> None:
    src = """
fn f() {
    view! {
        <button on:click=move |_| {
            set_foo.set(1);
        }>{"click"}</button>
    }
}
"""
    assert_violation_at("plain set inside on:click is flagged", src, 5)


def test_plain_set_inside_callback_new_is_flagged() -> None:
    src = """
fn f() {
    let cb = Callback::new(move |_: ()| {
        set_foo.set(false);
    });
}
"""
    assert_violation_at("plain set inside Callback::new is flagged", src, 4)


def test_rustfmt_split_multiline_is_flagged() -> None:
    src = """
fn f() {
    set_alert_data
        .set(Some(x));
}
"""
    assert_violation_at("rustfmt-split multi-line plain set is flagged", src, 3)


# ---------------------------------------------------------------------------
# True negatives — never flagged
# ---------------------------------------------------------------------------


def test_try_set_is_ok() -> None:
    src = """
fn f() {
    let _ = set_foo.try_set(1);
}
"""
    assert_no_violations("try_set is OK", src)


def test_try_update_is_ok() -> None:
    src = """
fn f() {
    let _ = set_foo.try_update(|v| *v = 1);
}
"""
    assert_no_violations("try_update is OK", src)


def test_try_set_untracked_is_ok() -> None:
    src = """
fn f() {
    let _ = set_foo.try_set_untracked(1);
}
"""
    assert_no_violations("try_set_untracked is OK", src)


def test_web_sys_setters_are_not_flagged() -> None:
    src = """
fn f() {
    window.set_interval_with_callback(cb, 1000);
    opts.set_body(&value);
    socket.set_onclose(Some(closure));
    headers.set("content-type", "application/json");
    element.set_onclick(handler);
}
"""
    assert_no_violations(
        "web_sys setters are not flagged (receiver is not a set_* identifier)",
        src,
    )


def test_disposal_safe_escape_hatch_respected() -> None:
    src = """
fn f() {
    set_foo.set(1); // disposal-safe: only runs during mount
}
"""
    assert_no_violations("// disposal-safe: escape hatch respected", src)


def test_line_comment_set_is_not_flagged() -> None:
    src = """
fn f() {
    // set_foo.set(1) — example in a comment
    let _ = set_foo.try_set(1);
}
"""
    assert_no_violations("signal write inside a // comment is not flagged", src)


def test_string_literal_set_is_not_flagged() -> None:
    src = '''
fn f() {
    let s = "set_foo.set(1)";
    let _ = set_foo.try_set(1);
}
'''
    assert_no_violations("signal write inside a string literal is not flagged", src)


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------


def main() -> int:
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    print(f"Running {len(tests)} tests for check_disposal_safety.py")
    for t in tests:
        try:
            t()
        except Exception as e:
            _failures.append((t.__name__, f"exception: {e}\n{traceback.format_exc()}"))
            print(f"  CRASH {t.__name__}: {e}")
    print()
    print(f"Passed: {_passed}")
    print(f"Failed: {len(_failures)}")
    if _failures:
        print()
        print("Failures:")
        for name, detail in _failures:
            print(f"  {name}: {detail}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Run the new self-tests against the OLD scanner to see them fail (red)**

Run:
```bash
python3 scripts/test_check_disposal_safety.py; echo "exit=$?"
```
Expected: `exit=1`. Specifically, the five new true-positive tests `test_plain_set_at_top_level_fn_is_flagged`, `test_plain_update_at_top_level_fn_is_flagged`, `test_plain_set_untracked_is_flagged`, `test_plain_update_untracked_is_flagged`, `test_plain_set_inside_on_click_is_flagged`, and `test_plain_set_inside_callback_new_is_flagged` fail because the current scanner only flags writes inside `spawn_local` / `Closure::wrap` danger zones, not writes in plain function bodies or event handlers.

- [ ] **Step 3: Do NOT commit yet**

The scanner rewrite in Task 3 will flip the tests to green. Keep the change staged locally but do not commit until Task 3 is ready to land in the same commit.

---

## Task 3: Rewrite scanner to context-free rule (TDD green)

**Files:**
- Modify: `scripts/check_disposal_safety.py`

- [ ] **Step 1: Replace the scanner with the context-free implementation**

Overwrite `scripts/check_disposal_safety.py` with:

```python
#!/usr/bin/env python3
"""CI gate: forbid plain Leptos signal writes (#153 hardening).

Context-free rule: any `.set()` / `.update()` / `.set_untracked()` /
`.update_untracked()` call on an identifier matching `set_\\w+` is a
violation anywhere in a .rs file under iem-mixer/iem-ui/src/. The only
safe variants are the `try_` prefixed forms, which return `Option<T>`
and silently no-op when the target signal has been disposed — exactly
the behavior we want defensively in a codebase that has background
timers, WebSocket callbacks, and async futures touching reactive state.

Why this rule is safe to enforce globally:

- The project convention for Leptos writer names is `set_<name>`. Web_sys
  setter methods like `window.set_interval_with_callback(...)`,
  `opts.set_body(...)`, `headers.set(...)`, and `element.set_onclick(...)`
  never appear on a receiver named `set_*`, so they do not match.
- Plain `.set()` and `.try_set()` have identical signatures except the
  return type. The only thing you lose by switching from `.set(x)` to
  `let _ = .try_set(x)` is the panic on disposed scopes, which is the
  whole point of the fix.
- Rustfmt sometimes splits long writes across two lines like
  `set_alert_data\\n    .set(Some(x))`. Those are caught by the
  `MULTILINE_WRITER_TAIL` + `MULTILINE_METHOD_HEAD` pair.

Escape hatch: append `// disposal-safe: <reason>` to any line the scanner
would otherwise flag. Use this only when you can prove the write cannot
race with scope disposal.

Usage: python3 scripts/check_disposal_safety.py
Exits 0 if clean, 1 if any violation is found.
"""

from __future__ import annotations

import pathlib
import re
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
SCAN_ROOT = REPO_ROOT / "iem-mixer" / "iem-ui" / "src"

# Match `set_<name>.<method>(` where method is one of the four unsafe
# variants. `\\btry_` would break the word boundary because `_` is a word
# character, so `\\btry_set` does NOT satisfy `\\bset_`. This is the
# mechanism that lets the regex distinguish `set_foo.set(1)` (unsafe)
# from `set_foo.try_set(1)` (safe).
UNSAFE_WRITE = re.compile(
    r"\bset_\w+\s*\.\s*\b(?:set|update)(?:_untracked)?\b\s*\("
)

# Rustfmt-split variants:
#     set_alert_data
#         .set(Some(x));
MULTILINE_WRITER_TAIL = re.compile(r"\bset_\w+\s*$")
MULTILINE_METHOD_HEAD = re.compile(
    r"^\.\s*\b(?:set|update)(?:_untracked)?\b\s*\("
)

ESCAPE_HATCH = "// disposal-safe:"


def _strip_strings(line: str) -> str:
    """Remove double-quoted string contents from a line (naive, good enough)."""
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
    """Return (line_number, line_text) for every violation in `path`."""
    violations: list[tuple[int, str]] = []
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    for lineno, line in enumerate(lines, start=1):
        stripped = line.strip()

        # Ignore pure line comments.
        if stripped.startswith("//"):
            continue

        stripped_code = _strip_strings(line)

        if UNSAFE_WRITE.search(stripped_code) and ESCAPE_HATCH not in line:
            violations.append((lineno, line.rstrip()))
            continue

        # Rustfmt-split: writer on this line, method head on the next.
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
            "::error::Plain Leptos signal writes are forbidden (disposal race, #153):"
        )
        for rel, lineno, line in all_violations:
            print(f"  {rel}:{lineno}: {line.strip()}")
        print()
        print("Fix: replace `.set(x)` with `let _ = .try_set(x)` and")
        print("     `.update(|v| ...)` with `let _ = .try_update(|v| ...)`.")
        print("     The try_ variants silently no-op on disposed signals,")
        print("     which is the whole point.")
        print()
        print("Escape hatch: append `// disposal-safe: <reason>` to the line")
        print("if you can prove the write cannot race with scope disposal.")
        return 1

    print(
        f"OK: no plain Leptos signal writes found "
        f"({SCAN_ROOT.relative_to(REPO_ROOT)}/**/*.rs)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Run the self-tests against the new scanner (green)**

Run:
```bash
python3 scripts/test_check_disposal_safety.py; echo "exit=$?"
```
Expected: `exit=0`, `Passed: 16`, `Failed: 0`.

- [ ] **Step 3: Run the scanner against the current (NOT yet swept) tree**

Run:
```bash
python3 scripts/check_disposal_safety.py; echo "exit=$?"
```
Expected: `exit=1` with ~255 violations printed across 14 files. This is EXPECTED — the sweep in Task 4 is what makes the gate pass.

- [ ] **Step 4: Commit the scanner + self-tests together**

```bash
git add scripts/check_disposal_safety.py scripts/test_check_disposal_safety.py
git commit -m "fix: context-free disposal safety scanner (#153)"
```

Do NOT push yet. The tree has ~255 scanner violations; push only after Task 4 lands them.

---

## Task 4: Project-wide try_set sweep

**Files:**
- Modify: all 14 files listed in the file map
- Create (NOT committed): `/tmp/sweep_try_set.py`

The sweep tool is a one-shot Python script that reads every `.rs` file under `iem-mixer/iem-ui/src/` and rewrites plain signal writes into their `try_` form. It is written to `/tmp/` and deleted after use — it does not belong in the repo.

- [ ] **Step 1: Write the sweep tool**

Create `/tmp/sweep_try_set.py` with:

```python
#!/usr/bin/env python3
"""One-shot project-wide try_set sweep for iem-mixer/iem-ui/src/.

Rewrites every `set_<name>.(set|update|set_untracked|update_untracked)(...)`
into `let _ = set_<name>.(try_set|try_update|try_set_untracked|try_update_untracked)(...)`.

Handles three forms:

1. Statement-level single-line write:
       set_foo.set(x);
   →
       let _ = set_foo.try_set(x);

2. Statement-level single-line update with closure:
       set_foo.update(|v| *v = x);
   →
       let _ = set_foo.try_update(|v| *v = x);

3. Rustfmt-split multi-line write:
       set_alert_data
           .set(Some(x));
   →
       let _ = set_alert_data
           .try_set(Some(x));

Handles nothing else. If the same site is used as an expression (rare in this
codebase, near zero occurrences), the scanner will still fail after the sweep
and the human reviewer must finish the conversion manually.

Preserves indentation by inserting `let _ = ` immediately after the leading
whitespace of the writer line.
"""

from __future__ import annotations

import pathlib
import re
import sys

SRC_ROOT = pathlib.Path("iem-mixer/iem-ui/src").resolve()

# Group 1: leading whitespace. Group 2: set_<name>. Group 3: method name.
# Group 4: `(` opening the args.
SINGLE_LINE = re.compile(
    r"^(\s*)(set_\w+)\s*\.\s*(set|update|set_untracked|update_untracked)\s*(\()"
)

# Matches a line that ends with `set_<name>` (rustfmt-split writer tail).
SPLIT_WRITER_TAIL = re.compile(r"^(\s*)(set_\w+)\s*$")
SPLIT_METHOD_HEAD = re.compile(
    r"^(\s*)\.\s*(set|update|set_untracked|update_untracked)\s*(\()"
)


def rewrite_method(method: str) -> str:
    return f"try_{method}"


def rewrite_file(path: pathlib.Path) -> int:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=False)
    newline = "\n" if text.endswith("\n") else ""
    out: list[str] = []
    changed = 0
    i = 0
    n = len(lines)
    while i < n:
        line = lines[i]

        # Rustfmt-split case FIRST so we can consume two lines together.
        m_tail = SPLIT_WRITER_TAIL.match(line)
        if m_tail and i + 1 < n:
            head = SPLIT_METHOD_HEAD.match(lines[i + 1])
            if head:
                lead, writer = m_tail.group(1), m_tail.group(2)
                head_lead, method, open_paren = (
                    head.group(1),
                    head.group(2),
                    head.group(3),
                )
                out.append(f"{lead}let _ = {writer}")
                new_method = rewrite_method(method)
                out.append(f"{head_lead}.{new_method}{open_paren}" + lines[i + 1][head.end():])
                changed += 1
                i += 2
                continue

        # Single-line case.
        m = SINGLE_LINE.match(line)
        if m:
            lead, writer, method, open_paren = (
                m.group(1),
                m.group(2),
                m.group(3),
                m.group(4),
            )
            rest = line[m.end():]
            new_method = rewrite_method(method)
            out.append(f"{lead}let _ = {writer}.{new_method}{open_paren}{rest}")
            changed += 1
            i += 1
            continue

        out.append(line)
        i += 1

    if changed:
        path.write_text("\n".join(out) + newline, encoding="utf-8")
    return changed


def main() -> int:
    total = 0
    touched = 0
    for rs in sorted(SRC_ROOT.rglob("*.rs")):
        c = rewrite_file(rs)
        if c:
            touched += 1
            total += c
            print(f"  {rs.relative_to(SRC_ROOT.parent.parent.parent)}: {c}")
    print(f"Touched {touched} files, rewrote {total} sites.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Run the sweep**

Run:
```bash
python3 /tmp/sweep_try_set.py
```
Expected: prints 14 files touched, ~255 total sites rewritten. The exact count must be between 250 and 260 — anything outside that range means the regex is matching too narrowly or too broadly.

- [ ] **Step 3: Spot-check one file**

Run:
```bash
grep -n 'let _ = set_ws.try_set\|set_ws.set' iem-mixer/iem-ui/src/pages/mixer.rs
```
Expected: the plain `set_ws.set(...)` at line 149 is gone; a new `let _ = set_ws.try_set(Some(ws.clone()));` exists in its place. No other `set_ws.set` lines should match.

- [ ] **Step 4: Run the scanner against the swept tree**

Run:
```bash
python3 scripts/check_disposal_safety.py; echo "exit=$?"
```
Expected: `exit=0` and `OK: no plain Leptos signal writes found`. If any violations remain, they are either (a) a sweep-tool bug where a closure form wasn't matched, or (b) a site where the write is used as an expression and must be fixed by hand. Read each remaining violation, fix it in the source, and re-run until green.

- [ ] **Step 5: Format the swept tree**

Run:
```bash
cd iem-mixer && cargo fmt --all && cd ..
```
Expected: may adjust whitespace in swept lines that now exceed 100 columns. No Rust compilation happens — `cargo fmt` is pure formatting.

- [ ] **Step 6: Delete the sweep tool**

Run:
```bash
rm /tmp/sweep_try_set.py
```

- [ ] **Step 7: Commit the sweep**

```bash
git add iem-mixer/iem-ui/src/
git commit -m "fix: project-wide try_set sweep for disposal safety (#153)"
```

---

## Task 5: Scanner gate verification

**Files:** none modified; this task is a pure verification checkpoint.

- [ ] **Step 1: Run scanner self-tests**

Run:
```bash
python3 scripts/test_check_disposal_safety.py
```
Expected: 16 passed, 0 failed.

- [ ] **Step 2: Run the scanner gate against the swept tree**

Run:
```bash
python3 scripts/check_disposal_safety.py
```
Expected: `exit=0`, `OK: no plain Leptos signal writes found (iem-mixer/iem-ui/src/**/*.rs)`.

- [ ] **Step 3: Sanity-grep the tree for residual violations**

Run:
```bash
grep -rn --include='*.rs' -E '\bset_\w+\s*\.\s*(set|update|set_untracked|update_untracked)\s*\(' iem-mixer/iem-ui/src/ | grep -v 'try_' | grep -v 'disposal-safe' | wc -l
```
Expected: `0`. If non-zero, the scanner has a false negative — read each residual line, fix the source, and loop back to Task 4 Step 4.

---

## Task 6: E2E test for navigation-back disposal

**Files:**
- Create: `iem-mixer/e2e/tests/live/navigation-back-disposal.spec.ts`

- [ ] **Step 1: Write the test**

Create `iem-mixer/e2e/tests/live/navigation-back-disposal.spec.ts` with:

```typescript
import { test, expect, Page, ConsoleMessage } from "@playwright/test";

/**
 * Post-deploy E2E for the navigation-back disposal race (#153 follow-up).
 *
 * Reproduces the user-reported Android PWA bug: after navigating back from
 * the member mixer to the member selector, the panic hook shows an error
 * page with "tried to access a reactive value that has already been
 * disposed". The underlying cause was plain `.set()` / `.update()` calls on
 * Leptos signals racing with component disposal. v1.144.0 hardened this by
 * converting every signal write to its `try_` variant and adding a
 * context-free CI scanner.
 *
 * Oracles (all three must pass for each scenario):
 *   1. No panic overlay is visible on the landing page after navigation.
 *   2. No `console.error` / `console.warn` messages during navigation
 *      and the 3-second settling window (matches the repo-wide
 *      browser-console-zero-errors contract).
 *   3. No POST to /api/client-error hits the server during the settling
 *      window (intercepted via page.route).
 *
 * Settling window: 3 seconds. The production logs showed the disposal
 * panic arriving at ~1 Hz (~2 full reconnect-interval ticks over 3s), so
 * 3s catches any leftover ghost interval that survived teardown.
 */

async function loginAs(page: Page, member: string, pin: string): Promise<void> {
  const response = await page.request.post("/api/auth", {
    data: { member, pin },
  });
  expect(response.status()).toBe(200);
  const data = await response.json();
  await page.evaluate(
    ({ token, member, engineer }) => {
      localStorage.setItem(
        "iem_token",
        JSON.stringify({ token, member, engineer }),
      );
    },
    { token: data.token, member: data.member, engineer: data.engineer },
  );
}

async function waitForStreamingMixer(page: Page): Promise<void> {
  // Mixer header visible (component mounted)
  await expect(page.locator(".mixer-header").first()).toBeVisible({
    timeout: 15000,
  });
  // Disconnected banner NOT visible (WebSocket is open)
  await expect(page.locator(".disconnected-banner")).not.toBeVisible({
    timeout: 15000,
  });
  // At least one channel rendered (State message arrived)
  await expect(page.locator(".channel-strip").first()).toBeVisible({
    timeout: 15000,
  });
  // Give the WebSocket a beat to receive a Meters frame so onmessage
  // has actually run at least once before we navigate away.
  await page.waitForTimeout(500);
}

function collectConsoleNoise(page: Page): { errors: string[]; warnings: string[] } {
  const errors: string[] = [];
  const warnings: string[] = [];
  page.on("console", (msg: ConsoleMessage) => {
    const type = msg.type();
    if (type === "error") errors.push(msg.text());
    else if (type === "warning") warnings.push(msg.text());
  });
  return { errors, warnings };
}

async function interceptClientErrorPosts(page: Page): Promise<string[]> {
  const posts: string[] = [];
  await page.route("**/api/client-error", async (route) => {
    const req = route.request();
    if (req.method() === "POST") {
      posts.push(req.postData() ?? "(empty)");
    }
    // Let the request continue so we also exercise the real endpoint.
    await route.continue();
  });
  return posts;
}

async function assertNoPanicOverlay(page: Page): Promise<void> {
  // The panic overlay injected by lifecycle::build_overlay_html contains a
  // known headline. If this selector is visible, the panic hook fired.
  const overlay = page.locator('body:has-text("Something went wrong")');
  await expect(overlay).toHaveCount(0);
}

async function assertCleanSettle(
  page: Page,
  console_noise: { errors: string[]; warnings: string[] },
  client_error_posts: string[],
  settleMs: number,
): Promise<void> {
  await page.waitForTimeout(settleMs);
  await assertNoPanicOverlay(page);
  const disposedNoise = [...console_noise.errors, ...console_noise.warnings].filter(
    (m) => m.toLowerCase().includes("disposed"),
  );
  expect(
    disposedNoise,
    `console had disposed-signal messages: ${JSON.stringify(disposedNoise)}`,
  ).toEqual([]);
  expect(
    client_error_posts,
    `unexpected /api/client-error POSTs during settle: ${JSON.stringify(
      client_error_posts,
    )}`,
  ).toEqual([]);
  expect(
    console_noise.errors,
    `console errors during settle: ${JSON.stringify(console_noise.errors)}`,
  ).toEqual([]);
  expect(
    console_noise.warnings,
    `console warnings during settle: ${JSON.stringify(console_noise.warnings)}`,
  ).toEqual([]);
}

test.describe("Navigation-back disposal race (#153 follow-up)", () => {
  test("mixer → landing via browser back: no panic, no console noise", async ({
    page,
  }) => {
    const noise = collectConsoleNoise(page);
    const posts = await interceptClientErrorPosts(page);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForStreamingMixer(page);

    // Navigate back via the browser's history API (matches Android back button).
    await page.goBack();
    await expect(page).toHaveURL(/\/$/);

    await assertCleanSettle(page, noise, posts, 3000);
  });

  test("mixer → landing via in-page back button: no panic", async ({
    page,
  }) => {
    const noise = collectConsoleNoise(page);
    const posts = await interceptClientErrorPosts(page);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForStreamingMixer(page);

    // The in-page back button is the .back-btn in mixer-header.
    await page.locator(".back-btn").click();
    await expect(page).toHaveURL(/\/$/);

    await assertCleanSettle(page, noise, posts, 3000);
  });

  test("mixer → different member's mixer: no panic", async ({ page }) => {
    const noise = collectConsoleNoise(page);
    const posts = await interceptClientErrorPosts(page);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForStreamingMixer(page);

    // Engineer can navigate directly to another member's mixer by URL.
    await page.goto("/petronela");
    await waitForStreamingMixer(page);

    await assertCleanSettle(page, noise, posts, 3000);
  });

  test("mixer → landing → mixer loop, 3 iterations: no panic accumulates", async ({
    page,
  }) => {
    const noise = collectConsoleNoise(page);
    const posts = await interceptClientErrorPosts(page);

    await page.goto("/");
    await loginAs(page, "engineer", "1177");

    for (let i = 0; i < 3; i++) {
      await page.goto("/engineer");
      await waitForStreamingMixer(page);
      await page.goBack();
      await expect(page).toHaveURL(/\/$/);
      // Short settle between iterations; the final 3s settle is at the end.
      await page.waitForTimeout(500);
    }

    await assertCleanSettle(page, noise, posts, 3000);
  });
});
```

- [ ] **Step 2: Commit the test**

```bash
git add iem-mixer/e2e/tests/live/navigation-back-disposal.spec.ts
git commit -m "test: navigation-back disposal E2E on live system (#153)"
```

---

## Task 7: Changelog entry

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add the v1.144.0 changelog entry**

Open `README.md`, find the section starting `## Changelog`, and insert immediately after that heading (above the `### v1.143.0` entry):

```markdown
### v1.144.0 (2026-04-12)

- **Fix**: Eliminated the "tried to access a reactive value that has already been disposed" error that could appear on the Android PWA when navigating back from the member mixer to the member selector. The underlying cause was plain Leptos `.set()` / `.update()` calls racing with component disposal when background intervals and WebSocket callbacks kept firing during teardown.
- **Hardening**: Swept every plain Leptos signal write in the UI to its defensive `try_set` / `try_update` variant (~255 call sites across 14 files). The `try_` variants silently no-op when the target signal has been disposed, which is exactly the desired behavior for background tasks and event handlers.
- **CI gate**: Rewrote `scripts/check_disposal_safety.py` into a context-free rule. Any plain `.set()` / `.update()` / `.set_untracked()` / `.update_untracked()` on an identifier matching `set_*` now fails the build. The scanner no longer needs to track danger zones — the rule is uniform across the codebase. Scanner self-tests (`scripts/test_check_disposal_safety.py`) were rewritten to match.
- **E2E**: New `iem-mixer/e2e/tests/live/navigation-back-disposal.spec.ts` test runs in the post-deploy job against the live system. Covers four navigation scenarios (browser back, in-page back button, mixer → mixer for a different member, and a 3-iteration loop) and asserts no panic overlay, no console errors/warnings, and no /api/client-error POSTs during a 3-second settling window after each navigation.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: changelog for v1.144.0 disposal-race deep fix (#153)"
```

---

## Task 8: Local fmt check, push, monitor CI

**Files:** none modified; push + CI only.

- [ ] **Step 1: Run cargo fmt check on the swept tree**

Run:
```bash
cd iem-mixer && cargo fmt --all --check && cd ..
```
Expected: exit 0, no output. If it fails, run `cd iem-mixer && cargo fmt --all && cd ..`, re-add the changed files, amend into the Task 4 commit — but since we commit-then-push, create a new `style: cargo fmt after sweep` commit instead.

- [ ] **Step 2: Push**

```bash
git push origin dev
```

- [ ] **Step 3: Monitor CI**

Run:
```bash
gh run list --branch dev --limit 3
```
Find the latest run, then poll until it reaches terminal state:
```bash
# Replace <run-id> with the actual id
gh run view <run-id>
```
Do NOT use `gh run watch`. If the run is still in progress, wait and re-check. All jobs must reach terminal state (success or failure) before proceeding.

Expected: all 10 jobs green (`Test Integrity Check`, `Lint & Format`, `Tests`, `Build WASM Frontend`, `Build VBAN VST3`, `Mutation Testing`, `Build Tauri (Windows)`, `E2E Tests`, `Deploy to iem.lan`, plus `Verify Version Bump` skipped on dev push).

- [ ] **Step 4: If CI fails, read the logs and fix**

Run:
```bash
gh run view <run-id> --log-failed
```
Common failure modes:

- **Test Integrity Check** fails on the scanner self-tests → scanner rewrite has a bug in the regex; fix `scripts/check_disposal_safety.py` and re-run `python3 scripts/test_check_disposal_safety.py` locally until green.
- **Test Integrity Check** fails on the scanner gate → the sweep missed a site; read the violation and fix the source.
- **Lint & Format** fails → run `cd iem-mixer && cargo fmt --all && cd ..`, commit the fmt changes as a new commit.
- **Tests** fails on a mutation-testing surviving-mutant → the scanner or swept code has a weak test; investigate before touching the rule.
- **E2E Tests** fails on `navigation-back-disposal.spec.ts` → the fix did NOT fully eliminate the panic. Read the console output in the test logs, identify which site still panics, and extend the sweep or add a manual fix. Do NOT loosen the test assertions.

Fix all issues in ONE commit, push once, re-monitor. No stream of "fix CI" commits.

---

## Task 9: Post-deploy verification on iem.lan

**Files:** none modified; verification only via MCP tools.

- [ ] **Step 1: Confirm the deploy landed**

Run:
```bash
curl -s http://10.77.9.231/api/version
```
Expected: JSON with `"version":"1.144.0"` and a fresh `deployed_at` timestamp within the last few minutes.

- [ ] **Step 2: Read the log file via the win-iem-snv MCP**

Use the `mcp__win-iem-snv__Shell` tool to run:
```powershell
$log = Get-ChildItem "$env:APPDATA\iem-mixer\logs\iem-mixer.log.*" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Select-String -Path $log.FullName -Pattern "client_error.*disposed" | Measure-Object | Select-Object -ExpandProperty Count
```
Expected: a number. Record it as the pre-verification baseline.

- [ ] **Step 3: Exercise the live scenario from the runner**

The E2E test in Task 6 already ran as part of the deploy job. Its post-condition is that zero new `/api/client-error` POSTs happen. Confirm that by running the MCP log count again 30 seconds after the E2E job completed:
```powershell
$log = Get-ChildItem "$env:APPDATA\iem-mixer\logs\iem-mixer.log.*" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Select-String -Path $log.FullName -Pattern "client_error.*disposed" | Measure-Object | Select-Object -ExpandProperty Count
```
Expected: the count is **equal to** the Step 2 baseline — no new disposed-signal panics were logged after the deploy. If the count increased, the fix did not fully hold. Read the new log entries via `Select-String ... | Select-Object -Last 5` to find out which panic slipped through.

- [ ] **Step 4: Verify the navigation-back scenario in Playwright against the live deployed system (manual run from the dev machine)**

Run:
```bash
cd iem-mixer && npx playwright test tests/live/navigation-back-disposal.spec.ts --project=chromium
```
Expected: all four scenarios pass.

---

## Task 10: Create PR, wait for explicit merge approval

**Files:** none modified.

- [ ] **Step 1: Create the PR from dev to main**

Run:
```bash
gh pr create --base main --head dev --title "Deeply fix Leptos disposal race on navigation back (#153 follow-up)" --body "$(cat <<'EOF'
## Summary

- Project-wide sweep of every plain Leptos `.set()` / `.update()` / `.set_untracked()` / `.update_untracked()` in `iem-mixer/iem-ui/src/` to the defensive `try_` variant (~255 sites across 14 files).
- Rewrote `scripts/check_disposal_safety.py` into a context-free CI gate: any plain `.set()` / `.update()` on a `set_*` identifier fails the build, no danger-zone tracking required.
- New post-deploy E2E `navigation-back-disposal.spec.ts` reproduces the Android PWA symptom against the live system with four scenarios and three oracles (no panic overlay, no console noise, no `/api/client-error` POSTs).
- Architectural alternative (approach C) tracked in #165 and explicitly out of scope here.

## Test plan

- [x] `scripts/test_check_disposal_safety.py` — 16/16 pass
- [x] `scripts/check_disposal_safety.py` — exit 0 against the swept tree
- [x] `cargo fmt --all --check` — clean
- [x] CI all 10 jobs green (incl. post-deploy E2E)
- [x] Post-deploy log: zero new `client_error.*disposed` entries after the v1.144.0 deploy timestamp
- [x] Manual Playwright run of `navigation-back-disposal.spec.ts` against the live system from the dev machine — all four scenarios pass

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Verify the PR is mergeable and clean**

Run:
```bash
gh pr view --json number,mergeable,mergeStateStatus,url
gh pr checks
```
Expected: `mergeable: true`, `mergeStateStatus: "CLEAN"`, all checks green.

- [ ] **Step 3: Provide the green PR URL and STOP**

Report the URL and all verification evidence in a completion report. Do NOT merge. Per the user's standing rule, a PR is merged only when the user says "merge it" (or equivalent explicit approval) in chat. Silence is not approval. Green CI is not permission.

---

## Verification (all must be green before the completion report is sent)

1. Scanner self-tests: 16/16 pass.
2. Scanner gate: exit 0.
3. `cargo fmt --all --check`: clean.
4. Dev push CI: all 10 jobs green.
5. Post-deploy E2E: `navigation-back-disposal.spec.ts` passes all four scenarios on the self-hosted runner.
6. Post-deploy log check: zero new `client_error.*disposed` entries after the deploy timestamp.
7. PR: mergeable, clean, all checks green.
8. User has been given the PR URL and the completion report contains every piece of evidence above.

Any item not green means the work is not done. Loop back and fix.
