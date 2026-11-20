#!/bin/bash
# Quick deploy script: SCP RPM to phone, install via devel-su, drop qmlcache, stop the app.
#
# Usage: ./quick-deploy.sh [HOST] [path/to/file.rpm]   (either order)
#   ./quick-deploy.sh                   # SFOS_HOST, auto-finds latest RPM
#   ./quick-deploy.sh myphone           # host or ssh alias as an argument
#   ./quick-deploy.sh myphone file.rpm  # Deploys specific RPM
#   ./quick-deploy.sh me@myphone        # ssh user too (overrides SFOS_USER)
#   SFOS_PASS=1234 ./quick-deploy.sh    # Non-interactive password
#   ./quick-deploy.sh --help            # This help
#
set -e

# Checked before any other argument, so --help works anywhere on the line.
for arg in "$@"; do
    case "$arg" in
        # Print the header comment above `set -e` as the help text.
        -h|--help) sed -n '2,/^set -e/{/^set -e/d;s/^# \{0,1\}//;p;}' "$0"; exit 0 ;;
    esac
done

ARG_HOST="" RPM=""
for arg in "$@"; do
    case "$arg" in
        *.rpm) [ -z "$RPM" ] || { echo "ERROR: more than one RPM given"; exit 1; }; RPM="$arg" ;;
        *)     [ -z "$ARG_HOST" ] || { echo "ERROR: more than one host given"; exit 1; }; ARG_HOST="$arg" ;;
    esac
done

HOST="${ARG_HOST:-${SFOS_HOST:-172.28.172.1}}"
USER="${SFOS_USER:-defaultuser}"
# user@host (as the argument or in SFOS_HOST) overrides SFOS_USER.
case "$HOST" in
    *@*) USER="${HOST%%@*}"; HOST="${HOST#*@}" ;;
esac
# SFOS_SSH_OPTS adds options, e.g. "-o StrictHostKeyChecking=no
# -o UserKnownHostsFile=/dev/null" for a phone whose host key changes on
# every reflash. Left unquoted where used so it splits into separate words.
SSH_OPTS="-o ConnectTimeout=10 ${SFOS_SSH_OPTS:-}"

# 1. Locate RPM
if [ -z "$RPM" ]; then
    # Look in current directory first, then mbuild.sh's OUT dir (same default)
    RPM="$(ls -t harbour-opencode-*.aarch64.rpm 2>/dev/null | head -1)"
    if [ -z "$RPM" ]; then
        OUT="${OUT:-${XDG_CACHE_HOME:-$HOME/.cache}/opencode-sailfish/out}"
        RPM="$(ls -t "$OUT"/harbour-opencode-*.aarch64.rpm 2>/dev/null | head -1)"
    fi
fi

if [ -z "$RPM" ] || [ ! -f "$RPM" ]; then
    echo "ERROR: No harbour-opencode RPM found. Build one first with ./mbuild.sh or specify path."
    exit 1
fi

echo "==> Deploying package: $RPM"
echo "==> Target: $USER@$HOST"

# 2. Check password for devel-su
if [ -z "$SFOS_PASS" ] && [ -t 0 ]; then
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
# The heredoc expands locally; \$ marks what the phone's shell evaluates.
ssh $SSH_OPTS "$USER@$HOST" bash -s <<EOF
set -e
rc=0
echo "$SFOS_PASS" | devel-su rpm -Uvh --force /tmp/harbour-opencode-deploy.rpm || rc=\$?
rm -f /tmp/harbour-opencode-deploy.rpm
[ \$rc -eq 0 ] || { echo "ERROR: install failed (exit \$rc)"; exit \$rc; }

echo "==> 3/4 Dropping qmlcache and terminating running instance..."
rm -rf /home/$USER/.cache/harbour-opencode/qmlcache
killall harbour-opencode 2>/dev/null || true

echo "==> 4/4 Verification:"
rpm -qa harbour-opencode
EOF

echo
echo "✓ Deploy completed successfully! You can launch harbour-opencode on your phone."
