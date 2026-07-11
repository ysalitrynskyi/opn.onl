#!/bin/sh
# Download GeoIP database from MaxMind
# Requires MAXMIND_ACCOUNT_ID and MAXMIND_LICENSE_KEY environment variables

set -e

GEOIP_DIR="/app/data"
GEOIP_FILE="$GEOIP_DIR/GeoLite2-City.mmdb"

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
DOWNLOAD_URL="https://download.maxmind.com/app/geoip_download?edition_id=GeoLite2-City&license_key=${MAXMIND_LICENSE_KEY}&suffix=tar.gz"

# Download and extract. GeoIP is optional, so any failure here (network blip,
# rate-limited/expired license key, or MaxMind changing the tarball layout) must
# degrade gracefully with exit 0 rather than aborting container startup. Under
# `set -e` a bare `wget`/`tar`/`mv` failure would exit non-zero *before* the
# graceful handling below ever runs, crash-looping the whole service — so each
# step is guarded by an `if !` condition (which `set -e` does not treat as fatal).
cd /tmp

if ! wget -q -O GeoLite2-City.tar.gz "$DOWNLOAD_URL"; then
    echo "Failed to download GeoIP database. Check your MaxMind credentials. Continuing without GeoIP."
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

rm -rf GeoLite2-City.tar.gz GeoLite2-City_*

echo "GeoIP database downloaded successfully to $GEOIP_FILE"





