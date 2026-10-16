#!/bin/sh
# Download GeoIP database from MaxMind
# Requires MAXMIND_ACCOUNT_ID and MAXMIND_LICENSE_KEY environment variables
#
# entrypoint.sh runs this before the server starts listening, and GeoIP is
# optional, so the download is bounded. Without a bound, a blackholed or
# stalled download.maxmind.com kept the API down (health refused, redirects
# failing) for as long as wget cared to wait: GNU wget defaults to 20 tries
# and a 900 s read timeout. Every failure, including running out of time,
# exits 0 and startup continues without GeoIP.

set -e

# The directory and URL can be overridden so test-download-geoip.sh can point
# this at a local listener. The two limits are in seconds.
GEOIP_DIR="${GEOIP_DIR:-/app/data}"
GEOIP_FILE="$GEOIP_DIR/GeoLite2-City.mmdb"
IDLE_SECS="${GEOIP_WGET_TIMEOUT:-10}"
BUDGET_SECS="${GEOIP_DOWNLOAD_BUDGET:-60}"

# Create directory if it doesn't exist
mkdir -p "$GEOIP_DIR"

# Check if already exists
if [ -f "$GEOIP_FILE" ]; then
    echo "GeoIP database already exists at $GEOIP_FILE"
    exit 0
fi

# Check for credentials
if [ -z "$MAXMIND_ACCOUNT_ID" ] || [ -z "$MAXMIND_LICENSE_KEY" ]; then
    echo "MAXMIND_ACCOUNT_ID or MAXMIND_LICENSE_KEY not set."
    echo "GeoIP features will be disabled."
    echo "To enable, sign up at https://www.maxmind.com/en/geolite2/signup"
    exit 0
fi

echo "Downloading GeoLite2-City database from MaxMind..."

# Download using MaxMind's direct download URL
DOWNLOAD_URL="${GEOIP_DOWNLOAD_URL:-https://download.maxmind.com/app/geoip_download?edition_id=GeoLite2-City&license_key=${MAXMIND_LICENSE_KEY}&suffix=tar.gz}"

# Work in a throwaway directory that is removed on every exit path, so a
# transfer that is cut off leaves no partial archive behind.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
trap 'exit 1' INT TERM
cd "$work"

# -T is the idle (connect/read) timeout and -t 1 makes it a single try; both
# flags exist in GNU wget and in busybox wget (which accepts and ignores -t).
# An idle timeout does not notice a server that keeps trickling bytes, so the
# transfer also gets a wall-clock budget: polled here because `timeout` is
# not on every machine the test runs on. The status of the failed `wait` is
# returned explicitly, so this does not depend on how a shell applies `set -e`
# inside a function.
download() {
    wget -q -T "$IDLE_SECS" -t 1 -O GeoLite2-City.tar.gz "$DOWNLOAD_URL" &
    wget_pid=$!
    waited=0
    while kill -0 "$wget_pid" 2>/dev/null; do
        if [ "$waited" -ge "$BUDGET_SECS" ]; then
            kill "$wget_pid" 2>/dev/null || true
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done
    wait "$wget_pid" || return 1
}

# Download and extract. GeoIP is optional, so any failure here (network blip,
# stall, rate-limited/expired license key, or MaxMind changing the tarball
# layout) must degrade gracefully with exit 0 rather than aborting container
# startup. Under `set -e` a bare `wget`/`tar`/`mv` failure would exit non-zero
# *before* the graceful handling below ever runs, crash-looping the whole
# service — so each step is guarded by an `if !` condition (which `set -e`
# does not treat as fatal).
if ! download; then
    echo "Failed to download GeoIP database. Check your MaxMind credentials and that download.maxmind.com is reachable. Continuing without GeoIP."
    exit 0
fi

if ! tar -xzf GeoLite2-City.tar.gz; then
    echo "Failed to extract GeoIP archive. Continuing without GeoIP."
    exit 0
fi

if ! mv GeoLite2-City_*/GeoLite2-City.mmdb "$GEOIP_FILE"; then
    echo "GeoIP archive did not contain the expected database file. Continuing without GeoIP."
    exit 0
fi

echo "GeoIP database downloaded successfully to $GEOIP_FILE"
