#!/bin/sh
set -e

# Replace runtime environment variables in index.html
# GA_ID - Google Analytics ID (e.g., G-XXXXXXXXXX)

INDEX_HTML="/usr/share/nginx/html/index.html"

# Replace GA_ID placeholder with actual value (or empty string if not set)
if [ -n "$GA_ID" ]; then
    echo "Injecting Google Analytics ID: $GA_ID"
    sed -i "s/%%GA_ID%%/$GA_ID/g" "$INDEX_HTML"
else
    echo "No GA_ID set, disabling Google Analytics"
    sed -i "s/%%GA_ID%%//g" "$INDEX_HTML"
fi

# Start nginx
exec "$@"



