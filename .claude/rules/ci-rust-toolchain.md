---
paths:
  - ".github/workflows/*.yml"
  - "iem-mixer/crates/**/*.rs"
---

# CI Rust toolchain gotchas

## `cargo install trunk` MUST be `--locked`

Unpinned, trunk resolves its own deps fresh each run. 2026-09-18 it pulled
`lightningcss 1.0.0-alpha.65` + `cssparser 0.37.0` → `E0277` inside trunk's
build, with zero iem-ui change. `ci.yml` + `release.yml` use `--locked`;
keep it for every cargo-installed CI tool.

## Clippy stable ≥ 1.98: `byte_char_slices`

`[b'O', b'I', b'E', b'M']` is denied → write `*b"OIEM"` (`audio_stream.rs`).
The runner tracks stable, so a toolchain bump can redden lint with no code
change — read the lint name in `--log-failed` before editing code.
