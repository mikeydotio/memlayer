#!/usr/bin/env bash
# E2E integration test: ingest → search → session summary round-trip
# Requires: docker-compose.test.yml stack running on port 8421
set -euo pipefail

API="http://127.0.0.1:8421/api"
TOKEN="test-e2e-token"
PASSED=0
FAILED=0

header() { printf "\n\033[1m=== %s ===\033[0m\n" "$1"; }
pass()   { PASSED=$((PASSED + 1)); printf "  \033[32m✓ %s\033[0m\n" "$1"; }
fail()   { FAILED=$((FAILED + 1)); printf "  \033[31m✗ %s\033[0m\n" "$1"; }

curl_api() {
    curl -sf -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" "$@"
}

# ---------------------------------------------------------------------------
header "Health check"
HEALTH=$(curl -sf http://127.0.0.1:8421/health)
if echo "$HEALTH" | grep -q '"status":"ok"'; then
    pass "Server is healthy"
else
    fail "Server health check failed: $HEALTH"
    exit 1
fi

# ---------------------------------------------------------------------------
header "Auth enforcement"
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:8421/api/search \
    -X POST -H "Content-Type: application/json" -d '{"query":"test"}')
if [ "$HTTP_CODE" = "401" ]; then
    pass "Unauthenticated request rejected (401)"
else
    fail "Expected 401 but got $HTTP_CODE"
fi

# ---------------------------------------------------------------------------
header "Ingest entries"
SESSION_ID="e2e-test-$(date +%s)"
NOW=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

INGEST_PAYLOAD=$(cat <<ENDJSON
{
  "entries": [
    {
      "payload_hash": "e2e-hash-001",
      "session_id": "$SESSION_ID",
      "message_type": "user",
      "content_type": "user",
      "raw_content": "How do I configure memlayer for production?",
      "timestamp": "$NOW",
      "project_path": "/home/e2e/test-project",
      "client_machine_id": "e2e-machine"
    },
    {
      "payload_hash": "e2e-hash-002",
      "session_id": "$SESSION_ID",
      "message_type": "assistant",
      "content_type": "assistant",
      "raw_content": "To configure memlayer for production, set MEMLAYER_AUTH_TOKEN, POSTGRES_PASSWORD, and optionally OPENAI_API_KEY in your .env file.",
      "timestamp": "$NOW",
      "project_path": "/home/e2e/test-project",
      "client_machine_id": "e2e-machine"
    },
    {
      "payload_hash": "e2e-hash-003",
      "session_id": "$SESSION_ID",
      "message_type": "user",
      "content_type": "user",
      "raw_content": "What about backup and restore?",
      "timestamp": "$NOW",
      "project_path": "/home/e2e/test-project",
      "client_machine_id": "e2e-machine"
    }
  ]
}
ENDJSON
)

INGEST_RESP=$(curl_api -X POST "$API/ingest" -d "$INGEST_PAYLOAD")
ACCEPTED=$(echo "$INGEST_RESP" | grep -o '"accepted":[0-9]*' | cut -d: -f2)
DUPES=$(echo "$INGEST_RESP" | grep -o '"duplicates":[0-9]*' | cut -d: -f2)

if [ "$ACCEPTED" = "3" ]; then
    pass "Ingested 3 entries (accepted=$ACCEPTED, duplicates=$DUPES)"
else
    fail "Expected 3 accepted, got: $INGEST_RESP"
fi

# ---------------------------------------------------------------------------
header "Idempotent re-ingest"
REINGEST_RESP=$(curl_api -X POST "$API/ingest" -d "$INGEST_PAYLOAD")
DUPES2=$(echo "$REINGEST_RESP" | grep -o '"duplicates":[0-9]*' | cut -d: -f2)
if [ "$DUPES2" = "3" ]; then
    pass "Re-ingest detected 3 duplicates"
else
    fail "Expected 3 duplicates, got: $REINGEST_RESP"
fi

# ---------------------------------------------------------------------------
header "Search (FTS-only, no embeddings)"
SEARCH_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"configure memlayer production","limit":5}')
TOTAL=$(echo "$SEARCH_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$TOTAL" -ge 1 ]; then
    pass "Search returned $TOTAL results for 'configure memlayer production'"
else
    fail "Expected >= 1 result, got: $SEARCH_RESP"
fi

# ---------------------------------------------------------------------------
header "Search with session filter"
FILTERED_RESP=$(curl_api -X POST "$API/search" \
    -d "{\"query\":\"memlayer\",\"session_id\":\"$SESSION_ID\",\"limit\":10}")
FILTERED_TOTAL=$(echo "$FILTERED_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$FILTERED_TOTAL" -ge 1 ]; then
    pass "Session-filtered search returned $FILTERED_TOTAL results"
else
    fail "Expected >= 1 session-filtered result, got total=$FILTERED_TOTAL"
fi

# ---------------------------------------------------------------------------
header "Search with type filter"
TYPE_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"memlayer","types":["assistant"],"limit":10}')
TYPE_TOTAL=$(echo "$TYPE_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$TYPE_TOTAL" -ge 1 ]; then
    pass "Type-filtered search returned $TYPE_TOTAL assistant results"
else
    fail "Expected >= 1 type-filtered result, got total=$TYPE_TOTAL"
fi

# ---------------------------------------------------------------------------
header "Session summary"
SUMMARY_RESP=$(curl_api "$API/sessions/$SESSION_ID/summary?limit=100")
MSG_COUNT=$(echo "$SUMMARY_RESP" | grep -o '"message_count":[0-9]*' | cut -d: -f2)

if [ "$MSG_COUNT" = "3" ]; then
    pass "Session summary has 3 messages"
else
    fail "Expected 3 messages, got: message_count=$MSG_COUNT"
fi

# ---------------------------------------------------------------------------
header "Response analytics in health"
HEALTH2=$(curl -sf http://127.0.0.1:8421/health)
if echo "$HEALTH2" | grep -q '"response_analytics"'; then
    pass "Health endpoint includes response analytics"
else
    fail "Missing response_analytics in health: $HEALTH2"
fi

# ---------------------------------------------------------------------------
header "Results"
echo ""
printf "  \033[32mPassed: %d\033[0m\n" "$PASSED"
printf "  \033[31mFailed: %d\033[0m\n" "$FAILED"
echo ""

if [ "$FAILED" -gt 0 ]; then
    exit 1
fi
echo "All E2E tests passed!"
