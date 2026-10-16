#!/usr/bin/env bash
# Tests for wait-for-ci.sh, with a fake `gh` that answers from a script of
# responses, one per call (the last one repeats). No network, no token.
#
# Run from anywhere: bash .github/scripts/test-wait-for-ci.sh
# Needs bash and jq.
set -euo pipefail

SCRIPT="$(cd "$(dirname "$0")" && pwd)/wait-for-ci.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
failures=0

mkdir -p "$WORK/bin"
cat >"$WORK/bin/gh" <<'GH'
#!/usr/bin/env bash
# Answer call N with $FAKE_GH_DIR/N (or the highest one there is); a file
# named N.fail makes call N fail like an API error.
n=$(( $(cat "$FAKE_GH_DIR/calls" 2>/dev/null || echo 0) + 1 ))
echo "$n" >"$FAKE_GH_DIR/calls"
if [ -f "$FAKE_GH_DIR/$n.fail" ]; then
    echo "gh: HTTP 502" >&2
    exit 1
fi
last=$(ls "$FAKE_GH_DIR" | grep -E '^[0-9]+$' | sort -n | tail -1)
k=$n
[ -f "$FAKE_GH_DIR/$k" ] || k=$last
cat "$FAKE_GH_DIR/$k"
GH
chmod +x "$WORK/bin/gh"

# run <name> <expected exit> <answer>... ; an answer is a list of
# "event:status:conclusion" runs separated by spaces, "-" for none, or "FAIL".
run() {
    local name="$1" want="$2"
    shift 2
    local dir="$WORK/$name"
    mkdir -p "$dir"
    local i=0
    for answer in "$@"; do
        i=$((i + 1))
        if [ "$answer" = FAIL ]; then
            : >"$dir/$i.fail"
            echo '{"workflow_runs":[]}' >"$dir/$i"
            continue
        fi
        local items="" id=100
        if [ "$answer" != "-" ]; then
            for r in $answer; do
                IFS=: read -r event status conclusion <<<"$r"
                id=$((id + 1))
                local c="null"
                [ "$conclusion" = "-" ] || c="\"$conclusion\""
                items="${items:+$items,}{\"id\":$id,\"event\":\"$event\",\"head_branch\":\"release\",\"status\":\"$status\",\"conclusion\":$c}"
            done
        fi
        echo "{\"workflow_runs\":[${items}]}" >"$dir/$i"
    done
    local got=0
    PATH="$WORK/bin:$PATH" FAKE_GH_DIR="$dir" CI_GATE_POLL=0 CI_GATE_APPEAR_TIMEOUT="${APPEAR:-600}" \
        CI_GATE_TIMEOUT="${TIMEOUT:-5400}" bash "$SCRIPT" owner/repo abc123 >"$dir/out" 2>&1 || got=$?
    if [ "$got" -ne "$want" ]; then
        echo "FAIL: $name: exit $got, wanted $want"
        sed 's/^/    /' "$dir/out"
        failures=$((failures + 1))
    else
        echo "ok: $name"
    fi
}

run "a passed push run lets the build go" 0 "push:completed:success"
run "a run still going is waited for" 0 "push:in_progress:-" "push:in_progress:-" "push:completed:success"
run "a failed run fails the gate" 1 "push:completed:failure"
run "a cancelled run alone fails the gate" 1 "push:completed:cancelled"
run "one branch cancelled, the other passed" 0 "push:completed:cancelled push:completed:success"
run "one branch failed, the other still going, then passing" 0 \
    "push:completed:failure push:in_progress:-" "push:completed:failure push:completed:success"
run "a pull_request run does not count" 1 "pull_request:completed:success push:completed:failure"
run "a manual CI run counts" 0 "workflow_dispatch:completed:success"
run "a run that appears late is waited for" 0 "-" "-" "push:queued:-" "push:completed:success"
APPEAR=0 run "no run at all fails once the appear timeout passes" 1 "-"
run "an API error is retried" 0 FAIL "push:completed:success"
TIMEOUT=0 run "the total timeout ends a run that never finishes" 1 "push:in_progress:-"

if [ "$failures" -ne 0 ]; then
    echo "$failures failure(s)"
    exit 1
fi
echo "wait-for-ci: all cases passed"
