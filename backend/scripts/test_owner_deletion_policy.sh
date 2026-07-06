#!/usr/bin/env bash
# Black-box regression test: deleting an org owner must not wipe the
# organization out from under its other members.
#
# Requires a running backend (fresh DB) started with
# ENABLE_ACCOUNT_DELETION=true. The first registered user becomes admin, so
# run this against a database with no users.
#
# Usage: BASE_URL=http://localhost:3111 ./test_owner_deletion_policy.sh [--expect-broken]
#
#   --expect-broken  Run only the destructive hard-delete scenario and REPORT
#                    (exit 0) if the org gets wiped — used to demonstrate the
#                    defect on a pre-fix build ("fails before").
set -u

BASE_URL="${BASE_URL:-http://localhost:3111}"
EXPECT_BROKEN=false
[ "${1:-}" = "--expect-broken" ] && EXPECT_BROKEN=true

PASS=0
FAIL=0
RESP=""
HTTP_STATUS=""

check() { # check <description> <actual> <expected>
    if [ "$2" = "$3" ]; then
        PASS=$((PASS+1)); echo "  ok: $1"
    else
        FAIL=$((FAIL+1)); echo "  FAIL: $1 (expected $3, got $2)"
    fi
}

req() { # req <method> <path> <token> [json-body] -> sets RESP and HTTP_STATUS
    local method=$1 path=$2 token=$3 body=${4:-}
    local args=(-s -w '\n%{http_code}' -X "$method" "$BASE_URL$path" -H 'Content-Type: application/json')
    [ -n "$token" ] && args+=(-H "Authorization: Bearer $token")
    [ -n "$body" ] && args+=(-d "$body")
    local out
    out=$(curl "${args[@]}")
    HTTP_STATUS=$(echo "$out" | tail -1)
    RESP=$(echo "$out" | sed '$d')
}

TS=$(date +%s)

echo "== setup: register admin (first user), owner, member =="
req POST /auth/register "" "{\"email\":\"admin-$TS@test.io\",\"password\":\"password123\"}"
ADMIN_TOKEN=$(echo "$RESP" | jq -r .token)
check "first user is admin" "$(echo "$RESP" | jq -r .is_admin)" "true"

req POST /auth/register "" "{\"email\":\"owner-$TS@test.io\",\"password\":\"password123\"}"
OWNER_TOKEN=$(echo "$RESP" | jq -r .token)
OWNER_ID=$(echo "$RESP" | jq -r .user_id)

req POST /auth/register "" "{\"email\":\"member-$TS@test.io\",\"password\":\"password123\"}"
MEMBER_TOKEN=$(echo "$RESP" | jq -r .token)
MEMBER_ID=$(echo "$RESP" | jq -r .user_id)

# Link creation requires a verified email; there is no SMTP in the test
# environment, so flip the flag directly when a DB URL is provided.
if [ -n "${TEST_DB_URL:-}" ]; then
    psql "$TEST_DB_URL" -qc "UPDATE users SET email_verified = true;"
    echo "  (marked test users email-verified via direct DB update)"
fi

echo "== setup: owner creates org, invites member; member adds an org link =="
req POST /orgs "$OWNER_TOKEN" "{\"name\":\"Team $TS\",\"slug\":\"team-$TS\"}"
ORG_ID=$(echo "$RESP" | jq -r .id)
check "org created" "$HTTP_STATUS" "201"

req POST "/orgs/$ORG_ID/members" "$OWNER_TOKEN" "{\"email\":\"member-$TS@test.io\",\"role\":\"admin\"}"
check "member invited" "$HTTP_STATUS" "201"

req POST /links "$MEMBER_TOKEN" "{\"original_url\":\"https://example.com/team-data\",\"org_id\":$ORG_ID}"
LINK_CODE=$(echo "$RESP" | jq -r .code)
if [ "$HTTP_STATUS" != "200" ] && [ "$HTTP_STATUS" != "201" ]; then
    FAIL=$((FAIL+1)); echo "  FAIL: member org link created (got $HTTP_STATUS)"
else
    PASS=$((PASS+1)); echo "  ok: member org link created ($LINK_CODE)"
fi

echo "== scenario 1: admin hard-deletes the org owner =="
req DELETE "/admin/users/$OWNER_ID/hard" "$ADMIN_TOKEN"
HARD_STATUS=$HTTP_STATUS

req GET "/orgs/$ORG_ID" "$MEMBER_TOKEN"
ORG_ALIVE_STATUS=$HTTP_STATUS
REDIRECT_STATUS=$(curl -s -o /dev/null -w '%{http_code}' "$BASE_URL/$LINK_CODE")

if $EXPECT_BROKEN; then
    echo "-- pre-fix build: demonstrating the defect --"
    check "hard delete succeeded (defect: no guard)" "$HARD_STATUS" "200"
    # The FK cascade deletes the org AND the membership rows, so the member
    # gets 403 "Not a member of this organization" instead of 404.
    check "org WIPED for member (defect)" "$ORG_ALIVE_STATUS" "403"
    check "member org link DEAD (defect)" "$REDIRECT_STATUS" "404"
    echo
    echo "defect reproduced: $PASS/$((PASS+FAIL)) observations matched the broken behavior"
    [ "$FAIL" -eq 0 ] || exit 1
    exit 0
fi

check "hard delete refused with 409" "$HARD_STATUS" "409"
check "org still visible to member" "$ORG_ALIVE_STATUS" "200"
if [ "$REDIRECT_STATUS" -ge 300 ] && [ "$REDIRECT_STATUS" -lt 400 ]; then
    PASS=$((PASS+1)); echo "  ok: member org link still redirects ($REDIRECT_STATUS)"
else
    FAIL=$((FAIL+1)); echo "  FAIL: member org link broken (got $REDIRECT_STATUS)"
fi

echo "== scenario 2: owner self-deletion refused while org has members =="
req POST /auth/delete-account "$OWNER_TOKEN" '{"password":"password123"}'
check "self-delete refused with 409" "$HTTP_STATUS" "409"
check "conflict code present" "$(echo "$RESP" | jq -r .code)" "ORG_OWNERSHIP_TRANSFER_REQUIRED"
check "conflict lists the org" "$(echo "$RESP" | jq -r '.organizations[0].id')" "$ORG_ID"

echo "== scenario 3: admin soft-delete refused while org has members =="
req DELETE "/admin/users/$OWNER_ID" "$ADMIN_TOKEN"
check "admin soft-delete refused with 409" "$HTTP_STATUS" "409"

echo "== scenario 4: transfer ownership, then self-delete succeeds =="
req POST "/orgs/$ORG_ID/transfer-ownership" "$OWNER_TOKEN" "{\"new_owner_user_id\":$MEMBER_ID}"
check "transfer accepted" "$HTTP_STATUS" "200"
check "org owner_id updated" "$(echo "$RESP" | jq -r .owner_id)" "$MEMBER_ID"

req POST /auth/delete-account "$OWNER_TOKEN" '{"password":"password123"}'
check "ex-owner self-delete now succeeds" "$HTTP_STATUS" "200"

req GET "/orgs/$ORG_ID" "$MEMBER_TOKEN"
check "org survives after ex-owner deleted" "$HTTP_STATUS" "200"
REDIRECT_STATUS=$(curl -s -o /dev/null -w '%{http_code}' "$BASE_URL/$LINK_CODE")
if [ "$REDIRECT_STATUS" -ge 300 ] && [ "$REDIRECT_STATUS" -lt 400 ]; then
    PASS=$((PASS+1)); echo "  ok: member org link still redirects after owner left"
else
    FAIL=$((FAIL+1)); echo "  FAIL: member org link broken after owner left (got $REDIRECT_STATUS)"
fi
req GET "/orgs/$ORG_ID/members" "$MEMBER_TOKEN"
check "new owner role recorded" "$(echo "$RESP" | jq -r ".[] | select(.user_id == $MEMBER_ID) | .role")" "owner"

echo "== scenario 5: solo org dies with its owner on hard delete =="
req POST /auth/register "" "{\"email\":\"solo-$TS@test.io\",\"password\":\"password123\"}"
SOLO_TOKEN=$(echo "$RESP" | jq -r .token)
SOLO_ID=$(echo "$RESP" | jq -r .user_id)
req POST /orgs "$SOLO_TOKEN" "{\"name\":\"Solo $TS\",\"slug\":\"solo-$TS\"}"

req DELETE "/admin/users/$SOLO_ID/hard" "$ADMIN_TOKEN"
check "solo owner hard delete succeeds" "$HTTP_STATUS" "200"
# The solo org row must be gone: recreating the same slug must succeed
# (slug is unique, so a 201 here proves the old row was deleted).
req POST /orgs "$MEMBER_TOKEN" "{\"name\":\"Solo reuse\",\"slug\":\"solo-$TS\"}"
check "solo org row gone (slug reusable)" "$HTTP_STATUS" "201"

echo
echo "result: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
