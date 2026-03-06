# Cloudflare Tunnel Setup for Mobile Access (Issue #24)

This document describes how to set up Cloudflare Tunnel to enable mobile data access to the IEM Mixer at `iem.newlevel.media`.

## Prerequisites

- Cloudflare account with `newlevel.media` domain
- iem.lan Windows machine with admin access
- Valid TLS certificate (already deployed via CI secrets)

## Installation

### 1. Install cloudflared on iem.lan

```powershell
# Download latest cloudflared
Invoke-WebRequest -Uri "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.msi" -OutFile "$env:TEMP\cloudflared.msi"

# Install
msiexec /i "$env:TEMP\cloudflared.msi" /quiet
```

### 2. Authenticate with Cloudflare

```powershell
cloudflared tunnel login
# Opens browser for authentication
```

### 3. Create the Tunnel

```powershell
cloudflared tunnel create iem-mixer
# Note the tunnel ID and credentials file path
```

### 4. Configure the Tunnel

Create `C:\Users\newlevel\.cloudflared\config.yml`:

```yaml
tunnel: iem-mixer
credentials-file: C:\Users\newlevel\.cloudflared\<tunnel-id>.json

ingress:
  - hostname: iem.newlevel.media
    service: http://localhost:80
  - service: http_status:404
```

### 5. Add DNS Record

```powershell
cloudflared tunnel route dns iem-mixer iem.newlevel.media
```

### 6. Run as Windows Service

```powershell
cloudflared service install
cloudflared service start
```

## Verification

1. Disable WiFi on your phone
2. Enable mobile data
3. Navigate to `https://iem.newlevel.media/`
4. The IEM Mixer landing page should load

## Troubleshooting

### Tunnel not connecting

```powershell
cloudflared tunnel info iem-mixer
cloudflared tunnel run iem-mixer  # Manual run for debugging
```

### Certificate issues

The app already serves HTTPS on port 443 with TLS certificates deployed via CI. Cloudflare Tunnel can connect to HTTP on port 80 and Cloudflare handles the HTTPS termination at the edge.

## Architecture

```
Mobile Phone (4G/5G)
        │
        ▼
Cloudflare Edge (HTTPS)
        │
        ▼
cloudflared tunnel (iem.lan)
        │
        ▼
IEM Mixer App (localhost:80)
```

## Notes

- Tunnel credentials are stored locally and should not be committed to git
- The tunnel runs as a Windows service and starts automatically on boot
- Traffic is encrypted end-to-end (Cloudflare uses TLS internally)
