#!/bin/bash
# Deploy web interface to REAPER on iem.lan

set -e

REMOTE_USER="newlevel"
REMOTE_HOST="iem.lan"
REMOTE_REAPER_WEB="/c/Program Files/REAPER (x64)/Data/web_interface"
LOCAL_WEB="./web/reaper_interface/"

echo "Deploying web interface to iem.lan..."

# Use rsync over SSH
rsync -avz --progress \
    "$LOCAL_WEB" \
    "${REMOTE_USER}@${REMOTE_HOST}:${REMOTE_REAPER_WEB}/iem_mixer/"

echo "Deploy complete!"
echo "Access at: http://iem.lan:8080/iem_mixer/iem_mixer.html"
