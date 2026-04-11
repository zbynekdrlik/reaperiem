#!/usr/bin/env python3
"""Unit tests for `scripts/check_disposal_safety.py`.

Run: python3 scripts/test_check_disposal_safety.py

The tests work on synthetic source snippets written to temporary files,
so they do not touch the real iem-ui tree. Each test creates a file,
runs `scan_file`, and asserts on the violations list.

Why unit test a grep-shaped tool: the scanner's entire value proposition
is that it has zero false positives on the existing safe patterns in the
codebase (so developers trust it and don't hit "clippy fatigue") and zero
false negatives on the known-bad patterns (so it actually prevents #153
regressions). A fixture-based test suite pins both sides.
"""

from __future__ import annotations

import pathlib
import sys
import tempfile
import traceback

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

# Import the module under test. This relies on check_disposal_safety.py
# being importable — it is when run as a script by file path.
import check_disposal_safety as cds  # noqa: E402


def scan_src(src: str) -> list[tuple[int, str]]:
    """Write `src` to a temp .rs file and run scan_file on it."""
    with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False) as f:
        f.write(src)
        path = pathlib.Path(f.name)
    try:
        return cds.scan_file(path)
    finally:
        path.unlink(missing_ok=True)


# ---------------------------------------------------------------------------
# Test helpers
# ---------------------------------------------------------------------------

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
# True positives — must be flagged
# ---------------------------------------------------------------------------


def test_set_inside_spawn_local_is_flagged() -> None:
    src = """
fn f() {
    spawn_local(async move {
        set_foo.set(1);
    });
}
"""
    assert_violation_at("set inside spawn_local is flagged", src, 4)


def test_update_inside_spawn_local_is_flagged() -> None:
    src = """
fn f() {
    spawn_local(async move {
        set_foo.update(|v| *v = 1);
    });
}
"""
    assert_violation_at("update inside spawn_local is flagged", src, 4)


def test_set_untracked_inside_spawn_local_is_flagged() -> None:
    src = """
fn f() {
    spawn_local(async move {
        set_foo.set_untracked(1);
    });
}
"""
    assert_violation_at("set_untracked inside spawn_local is flagged", src, 4)


def test_update_untracked_inside_spawn_local_is_flagged() -> None:
    src = """
fn f() {
    spawn_local(async move {
        set_foo.update_untracked(|v| *v = 1);
    });
}
"""
    assert_violation_at("update_untracked inside spawn_local is flagged", src, 4)


def test_set_inside_closure_wrap_is_flagged() -> None:
    src = """
fn f() {
    let cb = Closure::wrap(Box::new(move |_: Event| {
        set_state.set(Idle);
    }) as Box<dyn FnMut(_)>);
}
"""
    assert_violation_at("set inside Closure::wrap is flagged", src, 4)


def test_rustfmt_split_multiline_is_flagged() -> None:
    src = """
fn f() {
    spawn_local(async move {
        set_alert_data
            .set(Some(x));
    });
}
"""
    # The violation is reported at the WRITER line (first half), so fixers
    # land on the variable name.
    assert_violation_at("rustfmt-split multi-line is flagged", src, 4)


def test_nested_danger_zones_are_tracked_independently() -> None:
    src = """
fn f() {
    spawn_local(async move {
        let cb = Closure::wrap(Box::new(move |_: Event| {
            set_inner.set(1);
        }));
        set_outer.set(2);
    });
}
"""
    vs = scan_src(src)
    lines = sorted(ln for ln, _ in vs)
    check(
        "both inner and outer writes flagged",
        lines == [5, 7],
        f"expected [5, 7], got {lines}",
    )


# ---------------------------------------------------------------------------
# True negatives — must NOT be flagged
# ---------------------------------------------------------------------------


def test_try_set_inside_spawn_local_is_ok() -> None:
    src = """
fn f() {
    spawn_local(async move {
        let _ = set_foo.try_set(1);
    });
}
"""
    assert_no_violations("try_set inside spawn_local is OK", src)


def test_try_update_inside_spawn_local_is_ok() -> None:
    src = """
fn f() {
    spawn_local(async move {
        let _ = set_foo.try_update(|v| *v = 1);
    });
}
"""
    assert_no_violations("try_update inside spawn_local is OK", src)


def test_set_outside_any_danger_zone_is_ok() -> None:
    src = """
fn f() {
    set_foo.set(1);
    set_bar.update(|v| *v += 1);
}
"""
    assert_no_violations("set outside danger zone is OK", src)


def test_web_sys_setters_are_not_flagged() -> None:
    src = """
fn f() {
    spawn_local(async move {
        window.set_interval_with_callback(cb, 1000);
        opts.set_body(&value);
        socket.set_onclose(Some(closure));
        headers.set("content-type", "application/json");
        element.set_onclick(handler);
    });
}
"""
    assert_no_violations(
        "web_sys setters (set_interval, set_body, set_onclose, headers.set, set_onclick) are not flagged",
        src,
    )


def test_disposal_safe_escape_hatch_respected() -> None:
    src = """
fn f() {
    spawn_local(async move {
        set_foo.set(1); // disposal-safe: only runs during mount
    });
}
"""
    assert_no_violations("// disposal-safe: escape hatch respected", src)


def test_line_comment_set_is_not_flagged() -> None:
    src = """
fn f() {
    spawn_local(async move {
        // set_foo.set(1) — example in a comment
        let _ = set_foo.try_set(1);
    });
}
"""
    assert_no_violations("signal write inside a // comment is not flagged", src)


def test_string_literal_set_is_not_flagged() -> None:
    src = '''
fn f() {
    spawn_local(async move {
        let s = "set_foo.set(1)";
        let _ = set_foo.try_set(1);
    });
}
'''
    assert_no_violations("signal write inside a string literal is not flagged", src)


def test_set_in_plain_function_not_closure_not_flagged() -> None:
    src = """
fn f() {
    fn helper() {
        set_foo.set(1); // plain fn, not a closure
    }
    helper();
}
"""
    assert_no_violations("set inside a plain nested fn is not flagged", src)


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
