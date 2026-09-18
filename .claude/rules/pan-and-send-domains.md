---
paths:
  - "iem-mixer/crates/iem-server/src/*.rs"
  - "iem-mixer/crates/iem-core/src/{types,snapshot,preset}.rs"
---
# Pan domains + send_index on restore

**Pan:** poller converts REAPER→UI on read, so `Channel.pan`, cache, snapshots,
presets ALL hold **0..1 (0.5 = center)**. REAPER `SET/…/SEND/M/PAN` expects
**−1..1** → call `ui_pan_to_reaper` at the REAPER write only (WS `SetPan`,
`restore_send_pan` in both REST restores). #203: a raw write panned every mix
half-right. Exception: backup path is raw −1..1 end to end — leave it.

**send_index:** bulk writes (restore/replay) resolve per track via
`resolve_send_index` (discovered `mix_send_index`; `Err` if missing — SAFETY,
no fallback). Never hardcode 0 / the member's own index for mix channels. #204
