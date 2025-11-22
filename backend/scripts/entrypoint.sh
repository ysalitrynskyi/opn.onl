#!/bin/sh
# Docker entrypoint script
# Downloads GeoIP database if credentials are provided, then starts the app

set -e

# Download GeoIP database (optional)
/app/scripts/download-geoip.sh

# Start the application
exec /app/opn_onl_backend

