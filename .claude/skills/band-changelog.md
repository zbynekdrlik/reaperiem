---
name: band-changelog
description: Generate a band-member oriented changelog for IEM Mixer. Use when asked for changelog for band, kapela, or user-facing changes.
---

# Band Changelog Generator

Generate a **band-member oriented** changelog - NOT technical, NOT developer-focused.

## Rules

1. **Filter OUT** (band members don't care):
   - Version numbers in headers (only mention once at top)
   - CI/CD changes
   - Code refactoring
   - Security audits
   - MCP tools
   - Technical implementation details (WebSocket, ReaScript, etc.)
   - Developer tooling
   - Test changes

2. **Include ONLY** what band members actually use:
   - New buttons/features they can click
   - Changes to how controls work
   - Visual changes they see
   - Bug fixes that affected their experience
   - New ways to access the app

3. **Language**:
   - Slovak
   - Simple, non-technical
   - Short sentences
   - Use emojis for sections

4. **Check dates carefully**:
   - Only include changes since the specified period
   - Don't include features that already existed before

## Process

1. Get the date range (e.g., "last Sunday" = since that Sunday)
2. Run: `git log origin/main --since="YYYY-MM-DD" --oneline --no-merges`
3. Filter to user-facing changes only
4. Check when features were ACTUALLY added (not just fixed)
5. Write in the format below

## Output Format

```
Ahoj kapela! 👋

Novinky v IEM mixéri za tento týždeň (DD. – DD. mesiac YYYY).
Aktuálna verzia: X.Y.Z

📱 FEATURE TITLE
One-line description in simple Slovak.

💾 ANOTHER FEATURE
One-line description.

🔧 OPRAVY
• Fix 1
• Fix 2

Adresa: https://iem.newlevel.media/
```

## Section Emojis

- 📱 Mobile/access features
- 💾 Save/backup/history features
- ⚙️ Settings/configuration
- 🎨 Visual changes
- 🔧 Bug fixes
- 🎚️ Mixer controls
