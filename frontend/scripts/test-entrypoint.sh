#!/bin/sh
# Regression tests for docker-entrypoint.sh runtime injection.
#
# The entrypoint is what turns the %%GA_ID%% / %%GA_CONSENT_MODE%% placeholders in
# the built HTML into real values. It once substituted index.html only, which left
# every prerendered page (/features, /pricing, /login, …) serving the literal
# placeholder — Google Analytics silently never ran for anyone who landed on one.
# These tests pin that down, plus the input validation, without needing Docker.
set -eu

ENTRYPOINT="$(cd "$(dirname "$0")/.." && pwd)/docker-entrypoint.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

failures=0

fail() {
    echo "FAIL: $1"
    failures=$((failures + 1))
}

# A miniature dist/: the SPA shell plus a prerendered subroute, both carrying the
# same placeholders the real build emits.
make_root() {
    root="$WORK/$1"
    rm -rf "$root"
    mkdir -p "$root/features"
    for page in "$root/index.html" "$root/features/index.html"; do
        cat > "$page" <<'HTML'
<!doctype html>
<html><head>
<script>
  window.GA_ID = "%%GA_ID%%";
  window.GA_CONSENT_MODE = "%%GA_CONSENT_MODE%%";
</script>
</head><body><div id="root"></div></body></html>
HTML
    done
    echo "$root"
}

# assert_global <file> <js global> <expected value>
assert_global() {
    actual="$(sed -n "s/.*window\.$2 = \"\([^\"]*\)\".*/\1/p" "$1")"
    [ "$actual" = "$3" ] || fail "$1: window.$2 is '$actual', expected '$3'"
}

assert_no_placeholders() {
    if grep -rq '%%GA_' "$1"; then
        fail "$1: placeholders left unsubstituted in $(grep -rl '%%GA_' "$1" | tr '\n' ' ')"
    fi
}

echo "case: opt-out mode substitutes every HTML file, not just the SPA shell"
root="$(make_root optout)"
HTML_ROOT="$root" GA_ID=G-ABC123 GA_CONSENT_MODE=opt-out sh "$ENTRYPOINT" true > /dev/null
assert_no_placeholders "$root"
assert_global "$root/index.html" GA_ID G-ABC123
assert_global "$root/features/index.html" GA_ID G-ABC123
assert_global "$root/features/index.html" GA_CONSENT_MODE opt-out

echo "case: unset GA_CONSENT_MODE falls back to opt-in"
root="$(make_root default)"
HTML_ROOT="$root" GA_ID=G-ABC123 sh "$ENTRYPOINT" true > /dev/null
assert_global "$root/index.html" GA_CONSENT_MODE opt-in

echo "case: an unrecognized consent mode falls back to opt-in"
root="$(make_root typo)"
HTML_ROOT="$root" GA_ID=G-ABC123 GA_CONSENT_MODE=optout sh "$ENTRYPOINT" true > /dev/null
assert_global "$root/index.html" GA_CONSENT_MODE opt-in

echo "case: a malformed GA_ID is rejected rather than injected into the page"
root="$(make_root badid)"
HTML_ROOT="$root" GA_ID='G-X"; alert(1); //' sh "$ENTRYPOINT" true > /dev/null
assert_no_placeholders "$root"
assert_global "$root/index.html" GA_ID ""
if grep -q 'alert(1)' "$root/index.html"; then
    fail "malformed GA_ID was injected into the page"
fi

echo "case: no GA_ID at all disables analytics cleanly"
root="$(make_root noga)"
HTML_ROOT="$root" sh "$ENTRYPOINT" true > /dev/null
assert_no_placeholders "$root"
assert_global "$root/index.html" GA_ID ""
assert_global "$root/index.html" GA_CONSENT_MODE opt-in

if [ "$failures" -ne 0 ]; then
    echo "$failures check(s) failed"
    exit 1
fi
echo "docker-entrypoint.sh: all checks passed"
