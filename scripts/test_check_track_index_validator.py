#!/usr/bin/env python3
"""Self-tests for `scripts/check_track_index_validator.py` (#179 guard).

Ensures the scanner catches the known-bad patterns and does NOT
false-positive on comments, doc-comments, test-assertion strings, or
unrelated uses of `inputs.len()`.

Run: python3 scripts/test_check_track_index_validator.py
"""

from __future__ import annotations

import pathlib
import sys
import tempfile
import traceback

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import check_track_index_validator as ctv  # noqa: E402


def scan_src(src: str) -> list[tuple[int, str, str]]:
    with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False) as f:
        f.write(src)
        path = pathlib.Path(f.name)
    try:
        return ctv.scan_file(path)
    finally:
        path.unlink(missing_ok=True)


def test_catches_le_input_count() -> None:
    hits = scan_src(
        "let is_valid_track =\n"
        "    |ti: usize| -> bool { (ti >= 1 && ti <= input_count) || mix.contains(&ti) };\n"
    )
    assert len(hits) == 1, f"expected 1 hit, got {hits}"


def test_catches_le_inputs_len() -> None:
    hits = scan_src("if track_index > inputs.len() { return Err(..); }\n")
    assert len(hits) == 1, f"expected 1 hit, got {hits}"


def test_catches_validate_track_index_with_len() -> None:
    hits = scan_src(
        "validate_track_index(track_index, config.inputs.len())?;\n"
        "validate_track_index(ti, input_count)?;\n"
    )
    assert len(hits) == 2, f"expected 2 hits, got {hits}"


def test_ignores_comment_line() -> None:
    hits = scan_src(
        "// track_index <= input_count is banned per #179\n"
        "/// When track_index > inputs.len() we used to fail — now we don't\n"
    )
    assert hits == [], f"expected 0 hits, got {hits}"


def test_ignores_regression_test_string_literals() -> None:
    hits = scan_src(
        '    // A validator that used `ti <= inputs.len()` would silently reject\n'
        '    let msg = "regression: ti <= input_count would accept 2";\n'
    )
    assert hits == [], f"expected 0 hits, got {hits}"


def test_does_not_flag_unrelated_inputs_len() -> None:
    hits = scan_src(
        "for i in 0..inputs.len() { println!(\"{}\", i); }\n"
        "tracing::info!(count = inputs.len(), \"loaded\");\n"
    )
    assert hits == [], f"expected 0 hits, got {hits}"


def test_catches_inverse_form() -> None:
    hits = scan_src("if input_count < track_index { return Err(..); }\n")
    assert len(hits) == 1, f"expected 1 hit, got {hits}"


def test_catches_ge_inputs_len() -> None:
    hits = scan_src("if track_index >= inputs.len() { return Err(..); }\n")
    assert len(hits) == 1, f"expected 1 hit, got {hits}"


def run_all() -> int:
    tests = [
        test_catches_le_input_count,
        test_catches_le_inputs_len,
        test_catches_validate_track_index_with_len,
        test_ignores_comment_line,
        test_ignores_regression_test_string_literals,
        test_does_not_flag_unrelated_inputs_len,
        test_catches_inverse_form,
        test_catches_ge_inputs_len,
    ]
    failed = 0
    for t in tests:
        try:
            t()
            print(f"OK  {t.__name__}")
        except Exception:
            failed += 1
            print(f"FAIL {t.__name__}")
            traceback.print_exc()
    print(f"\n{len(tests) - failed}/{len(tests)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(run_all())
