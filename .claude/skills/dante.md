---
name: dante
description: Dante network audio control and safety boundaries. Use when working with network audio devices, channel naming, device discovery, or netaudio commands.
---

# Dante Network Audio Skill

## Network Topology

**Note:** Device names change when renamed. Always run `netaudio device list` to get current names.
See `config/dante_network.yaml` for current device names.

```
┌─────────────────────────────────────────────────────────────────────┐
│                        DANTE NETWORK                                │
├─────────────────────────────────────────────────────────────────────┤
│  [STAGEBOX]                       (32ch I/O)                        │
│  ├─ TX: Individual mics (vocals, instruments)                      │
│  └─ RX: IEM outputs to transmitters                                 │
│                                                                      │
│  [IEM ACCELERATOR]                (128ch)  ← YOUR DEVICE            │
│  ├─ TX: Personal IEM mixes (assigned to band members)              │
│  └─ RX: Inputs from stagebox + FOH stems                           │
│                                                                      │
│  [FOH ACCELERATOR]                (128ch)                           │
│  ├─ TX: House mixes, stems, monitoring                              │
│  └─ RX: Same inputs as IEM                                          │
└─────────────────────────────────────────────────────────────────────┘
```

## Device Roles

Identify devices by role, not name. Names may change.

| Role           | Channels | Claude Control | How to Identify             |
| -------------- | -------- | -------------- | --------------------------- |
| IEM mixing     | 128      | ✅ Full        | Connected to iem.lan REAPER |
| Stage inputs   | 32       | ❌ READ ONLY   | Has mic/DI channel names    |
| Front of house | 128      | ❌ READ ONLY   | Has "foh" in name typically |

## ⚠️ SAFETY BOUNDARIES (CRITICAL)

### ALLOWED Operations

```bash
# Device discovery
netaudio device list

# Channel information (ALL devices)
netaudio channel list --device-name <device>

# Subscription information
netaudio subscription list

# Channel naming (IEM DEVICE ONLY!)
# First get current IEM device name from: netaudio device list
netaudio config --device-name <IEM_DEVICE_NAME> --set-channel-name <ch> <name>
```

### NEVER DO These (WILL BREAK AUDIO!)

```bash
# ❌ NEVER modify subscriptions (breaks routing!)
netaudio subscription add/remove ...

# ❌ NEVER change device settings
netaudio config --set-sample-rate ...
netaudio config --set-latency ...
netaudio config --set-encoding ...

# ❌ NEVER modify stagebox or FOH (identify by role, not by name)
netaudio config --device-name <STAGEBOX_NAME> ...
netaudio config --device-name <FOH_NAME> ...
```

### Why These Restrictions?

1. **Subscriptions** define audio routing across the entire network. Breaking them silences the sound system.
2. **Sample rate/latency/encoding** must match across all devices. Changing one device desynchronizes the network.
3. **Stagebox/FOH** serve the entire venue, not just IEM. Changes affect the main PA system.

## Safe Commands Reference

| Command                                         | Purpose              | Safe?       |
| ----------------------------------------------- | -------------------- | ----------- |
| `netaudio device list`                          | List online devices  | ✅          |
| `netaudio channel list`                         | Show channel names   | ✅          |
| `netaudio subscription list`                    | Show routing         | ✅          |
| `netaudio config --identify`                    | Flash device LED     | ✅          |
| `netaudio config --set-channel-name` (IEM only) | Rename channel       | ⚠️ IEM ONLY |
| `netaudio subscription add`                     | Modify routing       | ❌ NEVER    |
| `netaudio config --set-*`                       | Change device config | ❌ NEVER    |

**Note:** Only powered-on devices appear in `netaudio device list`. Device availability varies.

## Band Member Channel Mappings

See `config/band_members.yaml` for current assignments.
Run `netaudio channel list --device-name <IEM_DEVICE>` to see actual channel names.

## Netaudio CLI Usage

```bash
# List all online Dante devices
netaudio device list

# List transmit channels from a device
netaudio channel list --device-name <DEVICE_NAME>

# See JSON output for scripting
netaudio channel list --device-name <DEVICE_NAME> --json

# Rename IEM output channel (SAFE - IEM device only)
# First get IEM device name from: netaudio device list
netaudio config --device-name <IEM_DEVICE_NAME> --set-channel-name 25 "MAREK L"
```

**Workflow:** Always run `netaudio device list` first to get current device names.

## Cross-References

- **REAPER control**: See `reaperiem` skill for track routing within REAPER
- **Hardware output routing**: Use MCP `set_hardware_output()` tool to route REAPER tracks to Dante channels
- **Configuration**: `config/dante_network.yaml` for device topology, `config/band_members.yaml` for member assignments

## Integration with REAPER

```
Dante Flow:
  Stagebox (TX) ──subscription──> IEM Accelerator (RX) ──ASIO──> REAPER inputs
                                                                      │
  REAPER outputs ──ASIO──> IEM Accelerator (TX) ──subscription──> Stagebox (RX)
                                                                      │
                                                              IEM transmitters
```

The MCP server controls:

1. REAPER track volumes/sends via HTTP API
2. REAPER hardware output routing via ReaScript
3. Channel naming on IEM Accelerator via netaudio

The MCP server does NOT control:

- Dante subscriptions (routing between devices)
- Audio flow on stagebox or FOH devices
