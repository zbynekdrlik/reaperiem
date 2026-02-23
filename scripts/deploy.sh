#!/bin/bash
# =============================================================================
# REAPERIEM DEPLOY SCRIPT
# =============================================================================
# Safely deploys code from dev machine to REAPER instances
#
# Usage:
#   ./deploy.sh              # Deploy to iem.lan
#   ./deploy.sh --status     # Show status
# =============================================================================

set -e

# Configuration
REMOTE_USER="newlevel"
REMOTE_HOST="iem.lan"
REPO_PATH='C:\Users\newlevel\Documents\reaperiem'
WWW_PATH='C:\Program Files\REAPER (x64)\Plugins\reaper_www_root'

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()   { echo -e "${GREEN}[DEPLOY]${NC} $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

cd "$(dirname "$0")/.."

case "${1:-}" in
    --status|-s)
        echo ""
        echo "========== STATUS =========="
        LOCAL=$(git rev-parse --short HEAD)
        echo "Local:    $LOCAL ($(git branch --show-current))"

        REMOTE=$(ssh ${REMOTE_USER}@${REMOTE_HOST} "cd ${REPO_PATH} && git rev-parse --short HEAD" 2>/dev/null || echo "N/A")
        DIRTY=$(ssh ${REMOTE_USER}@${REMOTE_HOST} "cd ${REPO_PATH} && git status --porcelain" 2>/dev/null | wc -l)

        if [[ "$LOCAL" == "$REMOTE" ]]; then
            echo -e "iem.lan:  $REMOTE ${GREEN}✓ IN SYNC${NC} ($DIRTY uncommitted)"
        else
            echo -e "iem.lan:  $REMOTE ${YELLOW}⚠ OUT OF SYNC${NC} ($DIRTY uncommitted)"
        fi
        echo "============================"
        ;;

    --help|-h)
        echo "Usage: $0 [--status|--help]"
        echo "  (no args)  Deploy to iem.lan"
        echo "  --status   Show sync status"
        ;;

    *)
        # DEPLOY
        log "Checking local status..."
        if [[ -n $(git status --porcelain) ]]; then
            error "Uncommitted local changes! Commit first."
        fi
        LOCAL_COMMIT=$(git rev-parse HEAD)

        log "Pushing to GitHub..."
        git push origin main || error "Push failed"

        log "Checking iem.lan for uncommitted changes..."
        REMOTE_DIRTY=$(ssh ${REMOTE_USER}@${REMOTE_HOST} "cd ${REPO_PATH} && git status --porcelain" 2>/dev/null || echo "")
        if [[ -n "$REMOTE_DIRTY" ]]; then
            warn "Auto-committing changes on iem.lan..."
            ssh ${REMOTE_USER}@${REMOTE_HOST} "cd ${REPO_PATH} && git add -A && git commit -m \"Auto-commit before deploy\"" || true
            ssh ${REMOTE_USER}@${REMOTE_HOST} "cd ${REPO_PATH} && git push origin main" || warn "Push failed"
        fi

        log "Saving rollback point..."
        ROLLBACK=$(ssh ${REMOTE_USER}@${REMOTE_HOST} "cd ${REPO_PATH} && git rev-parse HEAD")
        echo "    Rollback: $ROLLBACK"

        log "Pulling latest to iem.lan..."
        ssh ${REMOTE_USER}@${REMOTE_HOST} "cd ${REPO_PATH} && git pull origin main" || {
            error "Pull failed! Rollback: git reset --hard $ROLLBACK"
        }

        log "Deploying web interface..."
        scp web/reaper_interface/iem_mixer.html 'newlevel@iem.lan:C:/Program Files/REAPER (x64)/Plugins/reaper_www_root/iem_mixer.html'
        scp web/reaper_interface/iem.html 'newlevel@iem.lan:C:/Program Files/REAPER (x64)/Plugins/reaper_www_root/iem.html'

        log "Verifying..."
        FINAL=$(ssh ${REMOTE_USER}@${REMOTE_HOST} "cd ${REPO_PATH} && git rev-parse --short HEAD")

        echo ""
        echo -e "${GREEN}✅ DEPLOY COMPLETE${NC}"
        echo "    iem.lan @ $FINAL"
        echo "    Rollback: git reset --hard $ROLLBACK"
        ;;
esac
