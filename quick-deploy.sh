#!/bin/bash
# Quick deploy script: SCP RPM to phone, install via devel-su, drop qmlcache, kill booster/app.
#
# Usage:
#   ./quick-deploy.sh                   # Auto-finds latest RPM
#   ./quick-deploy.sh path/to/file.rpm  # Deploys specific RPM
#   SFOS_PASS=1234 ./quick-deploy.sh    # Non-interactive password
#
set -e

#HOST="${SFOS_HOST:-172.28.172.1}"
HOST="${SFOS_HOST:-192.168.76.232}"
USER="${SFOS_USER:-defaultuser}"
SSH_OPTS="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=10"

# 1. Locate RPM
RPM="$1"
if [ -z "$RPM" ]; then
    # Look in current directory first, then cache out dir
    RPM="$(ls -t harbour-opencode-*.aarch64.rpm 2>/dev/null | head -1)"
    if [ -z "$RPM" ]; then
        RPM="$(ls -t /home/jack/.cache/opencode-sailfish/out/harbour-opencode-*.aarch64.rpm 2>/dev/null | head -1)"
    fi
fi

if [ -z "$RPM" ] || [ ! -f "$RPM" ]; then
    echo "ERROR: No harbour-opencode RPM found. Build one first with ./mbuild.sh or specify path."
    exit 1
fi

echo "==> Deploying package: $RPM"
echo "==> Target: $USER@$HOST"

# 2. Check password for devel-su
if [ -z "$SFOS_PASS" ]; then
    read -s -p "Enter phone devel-su password: " SFOS_PASS
    echo
fi

if [ -z "$SFOS_PASS" ]; then
    echo "ERROR: SFOS_PASS is required for installing RPM on device."
    exit 1
fi

# 3. SCP RPM to device
echo "==> 1/4 Uploading RPM to phone..."
scp $SSH_OPTS "$RPM" "$USER@$HOST:/tmp/harbour-opencode-deploy.rpm"

# 4. Install & clear cache on device
echo "==> 2/4 Installing RPM on phone..."
ssh $SSH_OPTS "$USER@$HOST" bash -s <<EOF
set -e
echo "$SFOS_PASS" | devel-su rpm -Uvh --force /tmp/harbour-opencode-deploy.rpm
rm -f /tmp/harbour-opencode-deploy.rpm

echo "==> 3/4 Dropping qmlcache and terminating running instance..."
rm -rf /home/$USER/.cache/harbour-opencode/qmlcache
killall harbour-opencode 2>/dev/null || true
echo "$SFOS_PASS" | devel-su killall booster-silica-qt5 booster-qt5 2>/dev/null || true

echo "==> 4/4 Verification:"
rpm -qa harbour-opencode
EOF

echo
echo "✓ Deploy completed successfully! You can launch harbour-opencode on your phone."
