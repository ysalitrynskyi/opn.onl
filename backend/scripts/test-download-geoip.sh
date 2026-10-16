#!/bin/sh
# Regression tests for scripts/download-geoip.sh.
#
# The entrypoint runs that script before the server listens. wget used to run
# with no bound, so a blackholed download.maxmind.com kept health checks and
# redirects down until wget gave up on its own. These cases pin the bound (a
# listener that accepts and never answers must still exit 0 within a few
# seconds) and the paths that must keep working: skip when the database is
# already there, skip without credentials, install a real tarball, and give up
# cleanly on an archive that is not one.
#
# Needs sh, python3, tar and wget. Run from anywhere:
#   sh backend/scripts/test-download-geoip.sh
set -eu

SCRIPT="$(cd "$(dirname "$0")" && pwd)/download-geoip.sh"
WORK="$(mktemp -d)"
PIDS=""
failures=0

cleanup() {
    for pid in $PIDS; do
        kill "$pid" 2>/dev/null || true
    done
    rm -rf "$WORK"
}
trap cleanup EXIT INT TERM

fail() {
    echo "FAIL: $1"
    failures=$((failures + 1))
}

now() {
    python3 -c 'import time; print(time.time())'
}

wait_for_file() {
    i=0
    while [ ! -s "$1" ]; do
        i=$((i + 1))
        if [ "$i" -gt 100 ]; then
            echo "timed out waiting for $1" >&2
            exit 1
        fi
        sleep 0.1
    done
}

# Accept connections and never answer.
start_stall() {
    python3 - "$1" <<'PY' &
import socket, sys
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", 0))
s.listen(8)
with open(sys.argv[1], "w") as fh:
    fh.write(str(s.getsockname()[1]))
held = []
while True:
    conn, _ = s.accept()
    held.append(conn)
PY
    PIDS="$PIDS $!"
    wait_for_file "$1"
}

# Serve the file $2 for every GET.
start_file_server() {
    python3 - "$1" "$2" <<'PY' &
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

portfile, payload = sys.argv[1], sys.argv[2]

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        data = open(payload, "rb").read()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):
        pass

server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(portfile, "w") as fh:
    fh.write(str(server.server_address[1]))
server.serve_forever()
PY
    PIDS="$PIDS $!"
    wait_for_file "$1"
}

# run <geoip dir> <url> <idle secs> <budget secs> <kill after secs> [no-creds]
# Runs the script with that configuration and sets RC, ELAPSED and OUT. A run
# still going after <kill after> seconds is killed, so a lost bound fails the
# test instead of hanging CI.
run() {
    OUT="$WORK/out"
    started=$(now)
    if [ "${6:-}" = "no-creds" ]; then
        env -u MAXMIND_ACCOUNT_ID -u MAXMIND_LICENSE_KEY \
            GEOIP_DIR="$1" GEOIP_DOWNLOAD_URL="$2" \
            GEOIP_WGET_TIMEOUT="$3" GEOIP_DOWNLOAD_BUDGET="$4" \
            sh "$SCRIPT" >"$OUT" 2>&1 &
    else
        env MAXMIND_ACCOUNT_ID=acct MAXMIND_LICENSE_KEY=key \
            GEOIP_DIR="$1" GEOIP_DOWNLOAD_URL="$2" \
            GEOIP_WGET_TIMEOUT="$3" GEOIP_DOWNLOAD_BUDGET="$4" \
            sh "$SCRIPT" >"$OUT" 2>&1 &
    fi
    spid=$!
    (sleep "$5" && kill "$spid" 2>/dev/null) &
    guard=$!
    RC=0
    wait "$spid" || RC=$?
    kill "$guard" 2>/dev/null || true
    wait "$guard" 2>/dev/null || true
    ELAPSED=$(python3 -c "print(round($(now) - $started, 2))")
}

faster_than() {
    python3 -c "import sys; sys.exit(0 if float(sys.argv[1]) < float(sys.argv[2]) else 1)" "$ELAPSED" "$1"
}

stall_port="$WORK/stall.port"
start_stall "$stall_port"
stall_url="http://127.0.0.1:$(cat "$stall_port")/GeoLite2-City.tar.gz"

echo "case: a listener that accepts and never answers exits 0 within the budget"
# The idle timeout is longer than the budget, so only the wall-clock budget
# can end the transfer.
run "$WORK/stall" "$stall_url" 30 3 20
[ "$RC" -eq 0 ] || fail "stall: exit $RC: $(cat "$OUT")"
faster_than 12 || fail "stall: took ${ELAPSED}s, wanted under 12s"
grep -q "Continuing without GeoIP" "$OUT" || fail "stall: no continue message: $(cat "$OUT")"
[ ! -f "$WORK/stall/GeoLite2-City.mmdb" ] || fail "stall: wrote a database"

echo "case: the idle timeout alone also ends a stalled transfer"
run "$WORK/idle" "$stall_url" 2 60 20
[ "$RC" -eq 0 ] || fail "idle: exit $RC: $(cat "$OUT")"
faster_than 12 || fail "idle: took ${ELAPSED}s, wanted under 12s"

echo "case: an existing database is kept and nothing is downloaded"
mkdir -p "$WORK/existing"
printf 'already-there' >"$WORK/existing/GeoLite2-City.mmdb"
run "$WORK/existing" "$stall_url" 30 30 10
[ "$RC" -eq 0 ] || fail "existing: exit $RC"
faster_than 3 || fail "existing: took ${ELAPSED}s; the download was not skipped"
[ "$(cat "$WORK/existing/GeoLite2-City.mmdb")" = "already-there" ] || fail "existing: file replaced"

echo "case: missing credentials exit 0 without downloading"
run "$WORK/nocreds" "$stall_url" 30 30 10 no-creds
[ "$RC" -eq 0 ] || fail "no credentials: exit $RC"
faster_than 3 || fail "no credentials: took ${ELAPSED}s"
grep -q "GeoIP features will be disabled" "$OUT" || fail "no credentials: $(cat "$OUT")"

echo "case: a real tarball is installed"
mkdir -p "$WORK/pkg/GeoLite2-City_20261016"
printf 'mmdb-bytes' >"$WORK/pkg/GeoLite2-City_20261016/GeoLite2-City.mmdb"
(cd "$WORK/pkg" && tar -czf "$WORK/good.tar.gz" GeoLite2-City_20261016)
good_port="$WORK/good.port"
start_file_server "$good_port" "$WORK/good.tar.gz"
run "$WORK/good" "http://127.0.0.1:$(cat "$good_port")/x.tar.gz" 10 30 20
[ "$RC" -eq 0 ] || fail "tarball: exit $RC: $(cat "$OUT")"
[ "$(cat "$WORK/good/GeoLite2-City.mmdb" 2>/dev/null)" = "mmdb-bytes" ] \
    || fail "tarball: database not installed: $(cat "$OUT")"

echo "case: a download that is not an archive gives up cleanly"
printf 'not a tarball' >"$WORK/bad.tar.gz"
bad_port="$WORK/bad.port"
start_file_server "$bad_port" "$WORK/bad.tar.gz"
run "$WORK/bad" "http://127.0.0.1:$(cat "$bad_port")/x.tar.gz" 10 30 20
[ "$RC" -eq 0 ] || fail "bad archive: exit $RC"
grep -q "Continuing without GeoIP" "$OUT" || fail "bad archive: $(cat "$OUT")"
[ ! -f "$WORK/bad/GeoLite2-City.mmdb" ] || fail "bad archive: wrote a database"

if [ "$failures" -ne 0 ]; then
    echo "$failures failure(s)"
    exit 1
fi
echo "download-geoip: all cases passed"
