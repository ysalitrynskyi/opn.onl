#!/bin/sh
set -e

# Download GeoIP database (optional). Never let an optional-feature setup step
# abort startup: `|| true` keeps `set -e` from killing the container if the
# script ever exits non-zero for an unforeseen reason.
/app/scripts/download-geoip.sh || true

# Start the application
exec /app/opn_onl_backend
