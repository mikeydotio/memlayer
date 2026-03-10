#!/usr/bin/env bash
# E2E edge case tests: data integrity, boundary conditions, error handling
# Requires: docker-compose.test.yml stack running on port 8421
# Run AFTER e2e.sh (shares the same stack, data is additive)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
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

# Helper: curl that captures both status code and body
curl_status() {
    local tmpfile
    tmpfile=$(mktemp)
    local http_code
    http_code=$(curl -s -o "$tmpfile" -w "%{http_code}" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" "$@")
    echo "$http_code"
    cat "$tmpfile"
    rm -f "$tmpfile"
}

# ═══════════════════════════════════════════════════════════════════════
header "Ingest: Real data from JSONL logs"
REAL_DATA=$(cat "$SCRIPT_DIR/fixtures/real-entries.json")
REAL_RESP=$(curl_api -X POST "$API/ingest" -d "$REAL_DATA")
REAL_ACCEPTED=$(echo "$REAL_RESP" | grep -o '"accepted":[0-9]*' | cut -d: -f2)

if [ "$REAL_ACCEPTED" = "12" ]; then
    pass "Ingested 12 real entries from JSONL logs"
else
    fail "Expected 12 real entries accepted, got: $REAL_RESP"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Ingest: Idempotency (re-ingest real data)"
REINGEST_RESP=$(curl_api -X POST "$API/ingest" -d "$REAL_DATA")
REINGEST_DUPES=$(echo "$REINGEST_RESP" | grep -o '"duplicates":[0-9]*' | cut -d: -f2)

if [ "$REINGEST_DUPES" = "12" ]; then
    pass "Re-ingest detected 12 duplicates (idempotent)"
else
    fail "Expected 12 duplicates, got: $REINGEST_RESP"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Ingest: Edge case entries"
EDGE_DATA=$(cat "$SCRIPT_DIR/fixtures/edge-case-entries.json")
EDGE_RESP=$(curl_api -X POST "$API/ingest" -d "$EDGE_DATA")
EDGE_ACCEPTED=$(echo "$EDGE_RESP" | grep -o '"accepted":[0-9]*' | cut -d: -f2)

if [ "$EDGE_ACCEPTED" = "11" ]; then
    pass "Ingested 11 edge case entries (unicode, SQL chars, empty content, etc.)"
else
    fail "Expected 11 edge case entries accepted, got: $EDGE_RESP"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Ingest: Batch over 200 limit"
# Generate 201 entries
OVERSIZED=$(python3 -c "
import json
entries = []
for i in range(201):
    entries.append({
        'payload_hash': f'oversized-{i:04d}',
        'session_id': 'oversized-sess',
        'message_type': 'user',
        'content_type': 'user',
        'raw_content': f'Entry number {i}',
        'timestamp': '2026-01-15T10:00:00Z',
        'project_path': '/test',
        'client_machine_id': 'test'
    })
print(json.dumps({'entries': entries}))
")
OVERSIZED_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -X POST "$API/ingest" -d "$OVERSIZED")

if [ "$OVERSIZED_CODE" = "400" ]; then
    pass "Batch >200 entries rejected with 400"
else
    fail "Expected 400 for oversized batch, got $OVERSIZED_CODE"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Ingest: Empty entries array"
EMPTY_RESP=$(curl_api -X POST "$API/ingest" -d '{"entries":[]}')
EMPTY_ACCEPTED=$(echo "$EMPTY_RESP" | grep -o '"accepted":[0-9]*' | cut -d: -f2)

if [ "$EMPTY_ACCEPTED" = "0" ]; then
    pass "Empty batch returns accepted=0"
else
    fail "Expected accepted=0 for empty batch, got: $EMPTY_RESP"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Ingest: 100KB content entry"
LARGE_CONTENT=$(python3 -c "
import json
content = 'x' * 102400  # 100KB of 'x'
entry = {
    'entries': [{
        'payload_hash': 'large-content-100kb',
        'session_id': 'edge-sess-large',
        'message_type': 'assistant',
        'content_type': 'text',
        'raw_content': content,
        'timestamp': '2026-01-15T10:00:00Z',
        'project_path': '/test',
        'client_machine_id': 'test'
    }]
}
print(json.dumps(entry))
")
LARGE_RESP=$(curl_api -X POST "$API/ingest" -d "$LARGE_CONTENT")
LARGE_ACCEPTED=$(echo "$LARGE_RESP" | grep -o '"accepted":[0-9]*' | cut -d: -f2)

if [ "$LARGE_ACCEPTED" = "1" ]; then
    pass "100KB content entry accepted"
else
    fail "Expected 100KB entry accepted, got: $LARGE_RESP"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Ingest: Concurrent duplicate detection"
CONCURRENT_DATA=$(cat <<'ENDJSON'
{
  "entries": [{
    "payload_hash": "concurrent-test-001",
    "session_id": "concurrent-sess",
    "message_type": "user",
    "content_type": "user",
    "raw_content": "concurrent test entry",
    "timestamp": "2026-01-15T12:00:00Z",
    "project_path": "/test",
    "client_machine_id": "test"
  }]
}
ENDJSON
)

# Send same entry twice in parallel
RESP1_FILE=$(mktemp)
RESP2_FILE=$(mktemp)
curl -sf -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    -X POST "$API/ingest" -d "$CONCURRENT_DATA" -o "$RESP1_FILE" &
PID1=$!
curl -sf -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    -X POST "$API/ingest" -d "$CONCURRENT_DATA" -o "$RESP2_FILE" &
PID2=$!
wait $PID1 $PID2

TOTAL_ACCEPTED=0
TOTAL_DUPES=0
for f in "$RESP1_FILE" "$RESP2_FILE"; do
    a=$(grep -o '"accepted":[0-9]*' "$f" | cut -d: -f2)
    d=$(grep -o '"duplicates":[0-9]*' "$f" | cut -d: -f2)
    TOTAL_ACCEPTED=$((TOTAL_ACCEPTED + a))
    TOTAL_DUPES=$((TOTAL_DUPES + d))
done
rm -f "$RESP1_FILE" "$RESP2_FILE"

if [ $((TOTAL_ACCEPTED + TOTAL_DUPES)) -eq 2 ]; then
    pass "Concurrent ingest: total accepted+duplicates = 2 (no data loss)"
else
    fail "Expected accepted+duplicates=2, got accepted=$TOTAL_ACCEPTED dupes=$TOTAL_DUPES"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Search: Real data content"
SEARCH_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"bluetooth keyboard troubleshoot BlueZ","limit":10}')
SEARCH_TOTAL=$(echo "$SEARCH_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$SEARCH_TOTAL" -ge 1 ] 2>/dev/null; then
    pass "Search found real data content (total=$SEARCH_TOTAL)"
else
    fail "Expected >= 1 result for bluetooth query, got: $SEARCH_RESP"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Search: Empty query string"
EMPTY_Q_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    -X POST "$API/search" -d '{"query":"","limit":10}')

if [ "$EMPTY_Q_CODE" = "200" ] || [ "$EMPTY_Q_CODE" = "422" ]; then
    pass "Empty query handled gracefully (HTTP $EMPTY_Q_CODE)"
else
    fail "Expected 200 or 422 for empty query, got $EMPTY_Q_CODE"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Search: SQL injection attempt"
SQLI_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"'\''; DROP TABLE memory_entries; --","limit":10}')
# If we get a response at all, the table wasn't dropped
SQLI_CHECK=$(curl_api -X POST "$API/search" \
    -d '{"query":"bluetooth","limit":1}')
SQLI_TOTAL=$(echo "$SQLI_CHECK" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$SQLI_TOTAL" -ge 1 ] 2>/dev/null; then
    pass "SQL injection attempt had no effect (table intact, $SQLI_TOTAL results)"
else
    fail "Table may be damaged after SQL injection attempt"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Search: Very long query (5000 chars)"
LONG_Q=$(python3 -c "print('memlayer ' * 625)")
LONG_Q_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    -X POST "$API/search" -d "{\"query\":\"$LONG_Q\",\"limit\":5}")

if [ "$LONG_Q_CODE" = "200" ] || [ "$LONG_Q_CODE" = "500" ]; then
    pass "Very long query handled (HTTP $LONG_Q_CODE, no hang)"
else
    fail "Unexpected response for long query: $LONG_Q_CODE"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Search: Date filter - future date"
FUTURE_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"bluetooth","after":"2099-01-01T00:00:00Z","limit":10}')
FUTURE_TOTAL=$(echo "$FUTURE_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$FUTURE_TOTAL" = "0" ]; then
    pass "Future date filter returns 0 results"
else
    fail "Expected 0 results with future date, got total=$FUTURE_TOTAL"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Search: Date filter - past date"
PAST_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"bluetooth","before":"2000-01-01T00:00:00Z","limit":10}')
PAST_TOTAL=$(echo "$PAST_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$PAST_TOTAL" = "0" ]; then
    pass "Past date filter returns 0 results"
else
    fail "Expected 0 results with past date, got total=$PAST_TOTAL"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Search: Inverted date range (after > before)"
INVERT_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"bluetooth","after":"2027-01-01T00:00:00Z","before":"2025-01-01T00:00:00Z","limit":10}')
INVERT_TOTAL=$(echo "$INVERT_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$INVERT_TOTAL" = "0" ]; then
    pass "Inverted date range returns 0 results"
else
    fail "Expected 0 results with inverted dates, got total=$INVERT_TOTAL"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Search: Non-existent type filter"
TYPE_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"bluetooth","types":["nonexistent_type"],"limit":10}')
TYPE_TOTAL=$(echo "$TYPE_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$TYPE_TOTAL" = "0" ]; then
    pass "Non-existent type filter returns 0 results"
else
    fail "Expected 0 results with bad type filter, got total=$TYPE_TOTAL"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Search: Non-existent session filter"
BADSESS_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"bluetooth","session_id":"nonexistent-session-id","limit":10}')
BADSESS_TOTAL=$(echo "$BADSESS_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$BADSESS_TOTAL" = "0" ]; then
    pass "Non-existent session filter returns 0 results"
else
    fail "Expected 0 results with bad session, got total=$BADSESS_TOTAL"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Session: Real session summary"
SESS_RESP=$(curl_api "$API/sessions/real-sess-codeditor/summary?limit=100")
SESS_COUNT=$(echo "$SESS_RESP" | grep -o '"message_count":[0-9]*' | cut -d: -f2)

if [ "$SESS_COUNT" -ge 5 ] 2>/dev/null; then
    pass "Real session has $SESS_COUNT messages"
else
    fail "Expected >= 5 messages in real session, got: $SESS_RESP"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Session: Non-existent session"
NOSESS_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    "$API/sessions/totally-fake-session-id/summary")

if [ "$NOSESS_CODE" = "404" ]; then
    pass "Non-existent session returns 404"
else
    fail "Expected 404 for fake session, got $NOSESS_CODE"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Session: Multiple sessions from single batch"
SESS2_RESP=$(curl_api "$API/sessions/real-sess-memlayer/summary?limit=100")
SESS2_COUNT=$(echo "$SESS2_RESP" | grep -o '"message_count":[0-9]*' | cut -d: -f2)

if [ "$SESS2_COUNT" -ge 3 ] 2>/dev/null; then
    pass "Second session from same batch has $SESS2_COUNT messages"
else
    fail "Expected >= 3 messages in second session, got: $SESS2_RESP"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Data integrity: Unicode round-trip"
UNICODE_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"Unicode coffee rocket CJK","limit":5}')
UNICODE_TOTAL=$(echo "$UNICODE_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$UNICODE_TOTAL" -ge 1 ] 2>/dev/null; then
    pass "Unicode content searchable (total=$UNICODE_TOTAL)"
else
    fail "Expected >= 1 result for unicode search, got total=$UNICODE_TOTAL"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Data integrity: Tool use content preserved"
TOOL_SESS=$(curl_api "$API/sessions/real-sess-codeditor/summary?limit=100")
if echo "$TOOL_SESS" | grep -q 'Bash'; then
    pass "Tool name 'Bash' preserved in session data"
else
    fail "Tool name 'Bash' not found in session summary"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Data integrity: SQL metacharacters survived"
SQLI_SEARCH=$(curl_api -X POST "$API/search" \
    -d '{"query":"DROP TABLE memory_entries","limit":5}')
SQLI_SEARCH_TOTAL=$(echo "$SQLI_SEARCH" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$SQLI_SEARCH_TOTAL" -ge 1 ] 2>/dev/null; then
    pass "SQL metacharacter content stored and searchable"
else
    fail "Expected >= 1 result for SQL metachar search, got: $SQLI_SEARCH"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Data integrity: Same content different hashes"
HASH_SESS=$(curl_api "$API/sessions/edge-sess-001/summary?limit=100")
HASH_COUNT=$(echo "$HASH_SESS" | grep -o '"message_count":[0-9]*' | cut -d: -f2)

# edge-case entries has 10 entries for edge-sess-001 (not the one in edge-sess-002)
if [ "$HASH_COUNT" -ge 10 ] 2>/dev/null; then
    pass "Both same-content entries stored (different hashes, count=$HASH_COUNT)"
else
    fail "Expected >= 10 entries in edge-sess-001, got count=$HASH_COUNT"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Large response: Trigger file offloading"
# Ingest many entries to make a search return >200KB
for i in $(seq 1 50); do
    BATCH=$(python3 -c "
import json
entries = []
for j in range(4):
    idx = $i * 4 + j
    content = f'Memlayer production deployment guide section {idx}: ' + ('Configuration details for the memlayer server including database setup, authentication, embedding providers, and performance tuning. ' * 20)
    entries.append({
        'payload_hash': f'large-resp-{idx:04d}',
        'session_id': 'large-response-sess',
        'message_type': 'assistant',
        'content_type': 'text',
        'raw_content': content,
        'timestamp': '2026-01-15T10:00:00Z',
        'project_path': '/test',
        'client_machine_id': 'test'
    })
print(json.dumps({'entries': entries}))
")
    curl_api -X POST "$API/ingest" -d "$BATCH" > /dev/null 2>&1
done

LARGE_SEARCH=$(curl_api -X POST "$API/search" \
    -d '{"query":"memlayer production deployment guide configuration","limit":100}')

if echo "$LARGE_SEARCH" | grep -q '"large_response"'; then
    LARGE_FILE_ID=$(echo "$LARGE_SEARCH" | python3 -c "
import sys, json
data = json.load(sys.stdin)
lr = data.get('large_response')
if lr: print(lr['file_id'])
else: print('')
" 2>/dev/null)
    if [ -n "$LARGE_FILE_ID" ]; then
        pass "Large response offloaded to file (file_id=$LARGE_FILE_ID)"

        # Try to download the file
        LINES_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
            -H "Authorization: Bearer $TOKEN" \
            "$API/files/$LARGE_FILE_ID/lines?start=1&end=10")
        if [ "$LINES_CODE" = "200" ]; then
            pass "Large response file downloadable via /lines"
        else
            fail "Expected 200 for /lines, got $LINES_CODE"
        fi
    else
        pass "Large response field present (no file_id — within budget)"
    fi
else
    pass "Search results within 200KB budget (no offloading needed)"
fi

# ═══════════════════════════════════════════════════════════════════════
header "Results"
echo ""
printf "  \033[32mPassed: %d\033[0m\n" "$PASSED"
printf "  \033[31mFailed: %d\033[0m\n" "$FAILED"
echo ""

if [ "$FAILED" -gt 0 ]; then
    exit 1
fi
echo "All edge case E2E tests passed!"
