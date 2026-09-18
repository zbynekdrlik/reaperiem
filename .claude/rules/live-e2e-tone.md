---
paths:
  - "iem-mixer/e2e/tests/live/**"
  - ".github/workflows/ci.yml"
  - "scripts/reascripts/tone_generator.lua"
---
# Live audio E2E: tone generator

- "Tone active" indicator = **ENGINEER inear (track 32) peak**: `-1500` silent,
  `≈-224` tone. MASTER peak is useless (room-mic residual `≈-410` either way).
- `tone_generator.lua` = idempotent `start`/`stop`, NOT a toggle. Use the
  dynamically registered action id (as CI step "Activate tone generator").
- Meter specs call `expectSignalPresent()` (`mixer.spec.ts`): read peak → if
  silent re-fire `start` → re-read → assert with a NAMED message (fails in ~2 s
  as "precondition", never a 30 s meter timeout).
- Something removes the tone FX mid-suite (~min 20); culprit unpinned — #207.
