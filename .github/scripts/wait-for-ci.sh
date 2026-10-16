#!/usr/bin/env bash
# Wait until the CI workflow has passed on one commit.
#
# docker-build.yml runs this before it builds anything, so no image is
# published for a commit whose tests did not pass. Before it did, a push to
# `release` or a tag built and published `:latest` / `:X.Y.Z` while CI for the
# same commit was still running, and a red CI only showed up after the images
# were out.
#
# Only CI runs started by a push (or by hand) count: a pull_request run tests a
# merge ref, not the commit itself. The commit passes when any such run
# finished with success, so a run cancelled by a newer push to one branch does
# not block a commit that passed on the other. While a run is still queued or
# running, the script waits for it.
#
# Usage: wait-for-ci.sh <owner/repo> <sha>
# Needs `gh` (authenticated through GH_TOKEN) and `jq`.
#
# Settings, in seconds:
#   CI_GATE_TIMEOUT          give up after this long in total (default 5400)
#   CI_GATE_APPEAR_TIMEOUT   give up when no CI run exists after this long (default 600)
#   CI_GATE_POLL             time between two checks (default 30)
set -euo pipefail

repo="${1:?usage: wait-for-ci.sh <owner/repo> <sha>}"
sha="${2:?usage: wait-for-ci.sh <owner/repo> <sha>}"
timeout="${CI_GATE_TIMEOUT:-5400}"
appear_timeout="${CI_GATE_APPEAR_TIMEOUT:-600}"
poll="${CI_GATE_POLL:-30}"

start=$SECONDS
while :; do
    elapsed=$((SECONDS - start))
    # A failed API call is retried until the timeout, not taken as a verdict.
    if ! json="$(gh api "repos/${repo}/actions/workflows/ci.yml/runs?head_sha=${sha}&per_page=100")"; then
        if [ "$elapsed" -ge "$timeout" ]; then
            echo "::error::Could not read the CI runs for ${sha} within ${timeout}s."
            exit 1
        fi
        echo "Reading the CI runs failed (${elapsed}s); retrying."
        sleep "$poll"
        continue
    fi
    runs="$(echo "$json" | jq -r '.workflow_runs[]
        | select(.event == "push" or .event == "workflow_dispatch")
        | "\(.id) \(.event) \(.head_branch) \(.status) \(.conclusion // "-")"')"

    if [ -n "$runs" ]; then
        echo "CI runs for ${sha} after ${elapsed}s:"
        echo "  ${runs//$'\n'/$'\n'  }"
    fi

    if echo "$runs" | awk '$4 == "completed" && $5 == "success" { found = 1 } END { exit !found }'; then
        echo "CI passed on ${sha}."
        exit 0
    fi

    pending=$(echo "$runs" | awk 'NF && $4 != "completed" { n++ } END { print n + 0 }')
    if [ -n "$runs" ] && [ "$pending" -eq 0 ]; then
        echo "::error::CI did not pass on ${sha}: every run finished without success. Re-run CI for it, then re-run this workflow."
        exit 1
    fi
    if [ -z "$runs" ] && [ "$elapsed" -ge "$appear_timeout" ]; then
        echo "::error::No CI run started by a push exists for ${sha} after ${elapsed}s. Push the commit to main or release, or run CI on it by hand, then re-run this workflow."
        exit 1
    fi
    if [ "$elapsed" -ge "$timeout" ]; then
        echo "::error::CI on ${sha} did not finish within ${timeout}s."
        exit 1
    fi

    if [ -z "$runs" ]; then
        echo "No CI run for ${sha} yet (${elapsed}s); waiting."
    else
        echo "${pending} CI run(s) still going; waiting."
    fi
    sleep "$poll"
done
