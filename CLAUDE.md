# REAPER IEM Mixing System

<!-- airuleset:merge=manual -->

## Playbook router

- REAPER HTTP API / ReaScripts / EXTSTATE / iem.lan lifecycle → load `.claude/skills/reaper`
- Testing: live E2E safety / audio pipeline / post-deploy verification → load `.claude/skills/testing`
- Deployment: Tauri app / Windows runner / NSIS / backup-restore / VBAN / version files → load `.claude/skills/deployment`
- Dante network audio / netaudio CLI → load `.claude/skills/dante`
- IEM project overview / MCP tools / git workflow → load `.claude/skills/reaperiem`
- Band-member changelog → load `.claude/skills/band-changelog`

## Always-Apply Project Rules

### Git / file ownership (ENFORCED BY HOOKS)

| Files | Edit On | Commit On |
|-------|---------|-----------|
| Code (*.py, *.html, *.lua, *.rs) | Dev machine | Dev machine |
| REAPER projects (*.RPP) | iem.lan (REAPER) | iem.lan |

- NEVER `git add projects/*.RPP` on dev machine — hook rejects it
- NEVER use manual SCP/rsync — use `./scripts/deploy.sh`
- PR merge requires explicit user approval (<!-- airuleset:merge=manual --> is set above)

### CI constraints

- NEVER use `shell: bash` on self-hosted Windows runner (label: `iem-lan`)
- NEVER add `continue-on-error: true` — enforced by test-integrity CI job
- NEVER start new work while CI is running — monitor to terminal state first
- NEVER stop at partial CI green — own the entire pipeline including deploy

### REAPER safety

- NEVER `taskkill /F /IM reaper.exe` without saving first — has crashed remote Windows machine
- ALWAYS SAVE before restart: `curl "http://iem.lan:8080/_/40026"`
- NEVER modify Dante subscriptions or stagebox/FOH devices (see `.claude/skills/dante`)
- NEVER hardcode send_index=0 for mix channels on member inear tracks (see `.claude/skills/reaper`)

### Key commands

```bash
pytest                           # Run tests
python -m reaperiem_mcp.server   # Run MCP server locally
/mcp                             # Restart MCP connection in Claude Code
./scripts/deploy.sh              # Deploy code to iem.lan
```

### End-of-session

After completing any task that affects the IEM Mixer:
```
PR: <url> | CI: green | Deploy: verified | https://iem.newlevel.media/ (user-facing) | http://10.77.9.231/ (internal)
```

### Changelog

After EVERY PR merge to main, update README.md changelog with user-facing changes.
