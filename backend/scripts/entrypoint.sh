#!/bin/sh
set -e

# Download GeoIP database (optional)
/app/scripts/download-geoip.sh

# Start the application
exec /app/opn_onl_backend
