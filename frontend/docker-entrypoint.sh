#!/bin/sh
set -e

# Replace runtime environment variables in the served HTML.
#   GA_ID           - Google Analytics measurement ID (e.g. G-XXXXXXXXXX); unset = GA off
#   GA_CONSENT_MODE - "opt-out" (GA runs until the visitor declines) or
#                     "opt-in" / unset (GA waits for an explicit accept)
#
# Every prerendered page carries the same placeholders, not just the SPA shell:
# a visitor landing straight on /features or /pricing gets that file, so
# substituting only index.html left GA dead on every static route.

HTML_ROOT="${HTML_ROOT:-/usr/share/nginx/html}"

# GA measurement IDs are "G-" plus alphanumerics. Anything else is rejected
# rather than pasted into a <script> block verbatim.
GA_ID_VALUE=""
if [ -n "$GA_ID" ]; then
    if echo "$GA_ID" | grep -Eq '^G-[A-Za-z0-9]+$'; then
        GA_ID_VALUE="$GA_ID"
        echo "Injecting Google Analytics ID: $GA_ID_VALUE"
    else
        echo "WARNING: GA_ID '$GA_ID' is not a valid measurement ID (expected G-XXXXXXXXXX); disabling Google Analytics"
    fi
else
    echo "No GA_ID set, disabling Google Analytics"
fi

# Only the exact string "opt-out" flips the banner around; everything else
# (including a typo) stays on the safe opt-in default.
if [ "$GA_CONSENT_MODE" = "opt-out" ]; then
    GA_CONSENT_MODE_VALUE="opt-out"
else
    GA_CONSENT_MODE_VALUE="opt-in"
    if [ -n "$GA_CONSENT_MODE" ] && [ "$GA_CONSENT_MODE" != "opt-in" ]; then
        echo "WARNING: GA_CONSENT_MODE '$GA_CONSENT_MODE' is not recognized (expected opt-in or opt-out); using opt-in"
    fi
fi
if [ -n "$GA_ID_VALUE" ]; then
    echo "Analytics consent mode: $GA_CONSENT_MODE_VALUE"
fi

# Substitute via a temp file rather than `sed -i`: the in-place flag is spelled
# differently on GNU and BSD sed, and this way the script can also be exercised
# outside the container. Writing back with `cat >` keeps each file's original
# owner and mode.
TMP_HTML="$(mktemp)"
trap 'rm -f "$TMP_HTML"' EXIT
find "$HTML_ROOT" -name '*.html' -type f | while IFS= read -r html; do
    sed -e "s/%%GA_ID%%/$GA_ID_VALUE/g" \
        -e "s/%%GA_CONSENT_MODE%%/$GA_CONSENT_MODE_VALUE/g" \
        "$html" > "$TMP_HTML"
    cat "$TMP_HTML" > "$html"
done
# `exec` below replaces this shell, so the EXIT trap never fires — clean up here.
rm -f "$TMP_HTML"

# Start nginx
exec "$@"
