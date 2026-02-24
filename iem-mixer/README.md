# IEM Mixer

Desktop application for in-ear monitor mixing with REAPER.

## Features

- Simple URLs: `iem.lan/petka` instead of complex query params
- PIN authentication per band member + engineer PIN
- Dark theme optimized for stage use
- System tray with member quick access
- Auto-updater for easy updates

## Architecture

- **Tauri 2.0** - Desktop shell with system tray
- **Leptos CSR/WASM** - Client-side rendering for offline capability
- **Axum** - API server embedded in binary

## Build

Builds run on GitHub Actions. See `.github/workflows/ci.yml`.

## Configuration

Copy `config/config.example.yaml` to your config directory:

- Windows: `%APPDATA%/iem-mixer/config.yaml`
- Linux: `~/.config/iem-mixer/config.yaml`

## License

MIT
