#!/bin/bash
# =============================================================================
# SETUP GIT HOOKS
# =============================================================================
# Installs the appropriate pre-commit hook based on machine type
#
# Usage:
#   ./setup-hooks.sh dev    # On dev machine
#   ./setup-hooks.sh iem    # On iem.lan
# =============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOKS_DIR="$REPO_ROOT/.git/hooks"

case "${1:-}" in
    dev)
        echo "Installing dev machine pre-commit hook..."
        cp "$SCRIPT_DIR/hooks/pre-commit-dev" "$HOOKS_DIR/pre-commit"
        chmod +x "$HOOKS_DIR/pre-commit"
        echo "✅ Hook installed. This machine will REJECT commits to:"
        echo "   - projects/*.RPP"
        echo "   - Any REAPER project files"
        ;;
    iem)
        echo "Installing iem.lan pre-commit hook..."
        cp "$SCRIPT_DIR/hooks/pre-commit-iem" "$HOOKS_DIR/pre-commit"
        chmod +x "$HOOKS_DIR/pre-commit"
        echo "✅ Hook installed. This machine will REJECT commits to:"
        echo "   - mcp/* (Python code)"
        echo "   - web/* (HTML/JS)"
        echo "   - scripts/* (Shell scripts)"
        echo "   - Any code files"
        echo ""
        echo "   This machine can ONLY commit:"
        echo "   - projects/*.RPP"
        ;;
    status)
        if [[ -f "$HOOKS_DIR/pre-commit" ]]; then
            if grep -q "DEV MACHINE" "$HOOKS_DIR/pre-commit"; then
                echo "Current hook: DEV (rejects RPP files)"
            elif grep -q "IEM.LAN" "$HOOKS_DIR/pre-commit"; then
                echo "Current hook: IEM (rejects code files)"
            else
                echo "Current hook: UNKNOWN (custom)"
            fi
        else
            echo "No pre-commit hook installed"
        fi
        ;;
    *)
        echo "Usage: $0 <dev|iem|status>"
        echo ""
        echo "  dev     Install hook for dev machine (rejects RPP files)"
        echo "  iem     Install hook for iem.lan (rejects code files)"
        echo "  status  Show current hook status"
        exit 1
        ;;
esac
