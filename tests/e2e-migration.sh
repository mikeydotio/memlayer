#!/usr/bin/env bash
# E2E migration test: full source → destination migration flow
# Requires: docker-compose.migration-test.yml stack running
#   source-server on port 8423, dest-server on port 8424
set -euo pipefail

SOURCE="http://127.0.0.1:8423/api"
DEST="http://127.0.0.1:8424/api"
SOURCE_TOKEN="source-token"
DEST_TOKEN="dest-token"
# Docker network URL for dest-server to reach source-server
SOURCE_INTERNAL="http://source-server:8420/api"
PASSED=0
FAILED=0

header() { printf "\n\033[1m=== %s ===\033[0m\n" "$1"; }
pass()   { PASSED=$((PASSED + 1)); printf "  \033[32m✓ %s\033[0m\n" "$1"; }
fail()   { FAILED=$((FAILED + 1)); printf "  \033[31m✗ %s\033[0m\n" "$1"; }

curl_source() {
    curl -sf -H "Authorization: Bearer $SOURCE_TOKEN" -H "Content-Type: application/json" "$@"
}

curl_dest() {
    curl -sf -H "Authorization: Bearer $DEST_TOKEN" -H "Content-Type: application/json" "$@"
}

# ---------------------------------------------------------------------------
header "Health checks"
SOURCE_HEALTH=$(curl -sf http://127.0.0.1:8423/health)
if echo "$SOURCE_HEALTH" | grep -q '"status":"ok"'; then
    pass "Source server is healthy"
else
    fail "Source server health check failed: $SOURCE_HEALTH"
    exit 1
fi

DEST_HEALTH=$(curl -sf http://127.0.0.1:8424/health)
if echo "$DEST_HEALTH" | grep -q '"status":"ok"'; then
    pass "Destination server is healthy"
else
    fail "Destination server health check failed: $DEST_HEALTH"
    exit 1
fi

# ---------------------------------------------------------------------------
header "Seed source with test data"
NOW=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
SESSION_ID="migration-test-$(date +%s)"

INGEST_PAYLOAD=$(cat <<ENDJSON
{
  "entries": [
    {
      "payload_hash": "mig-hash-001",
      "session_id": "$SESSION_ID",
      "message_type": "user",
      "content_type": "user",
      "raw_content": "How do I configure memlayer for production?",
      "timestamp": "$NOW",
      "project_path": "/home/test/project",
      "client_machine_id": "test-machine"
    },
    {
      "payload_hash": "mig-hash-002",
      "session_id": "$SESSION_ID",
      "message_type": "assistant",
      "content_type": "assistant",
      "raw_content": "Set MEMLAYER_AUTH_TOKEN and POSTGRES_PASSWORD in your .env file.",
      "timestamp": "$NOW",
      "project_path": "/home/test/project",
      "client_machine_id": "test-machine"
    },
    {
      "payload_hash": "mig-hash-003",
      "session_id": "$SESSION_ID",
      "message_type": "user",
      "content_type": "user",
      "raw_content": "What about backup and restore procedures?",
      "timestamp": "$NOW",
      "project_path": "/home/test/project",
      "client_machine_id": "test-machine"
    }
  ]
}
ENDJSON
)

INGEST_RESP=$(curl_source -X POST "$SOURCE/ingest" -d "$INGEST_PAYLOAD")
ACCEPTED=$(echo "$INGEST_RESP" | grep -o '"accepted":[0-9]*' | cut -d: -f2)
if [ "$ACCEPTED" = "3" ]; then
    pass "Seeded source with 3 entries"
else
    fail "Expected 3 accepted, got: $INGEST_RESP"
    exit 1
fi

# ---------------------------------------------------------------------------
header "Step 1: Initiate migration on source"
INITIATE_RESP=$(curl_source -X POST "$SOURCE/migration/initiate")
MIGRATION_ID=$(echo "$INITIATE_RESP" | grep -o '"migration_id":"[^"]*"' | cut -d'"' -f4)
MIGRATION_KEY=$(echo "$INITIATE_RESP" | grep -o '"migration_key":"[^"]*"' | cut -d'"' -f4)
INIT_STATE=$(echo "$INITIATE_RESP" | grep -o '"state":"[^"]*"' | cut -d'"' -f4)

if [ "$INIT_STATE" = "INITIATED" ] && [ -n "$MIGRATION_KEY" ]; then
    pass "Migration initiated (id=$MIGRATION_ID)"
else
    fail "Expected INITIATED state, got: $INITIATE_RESP"
    exit 1
fi

# ---------------------------------------------------------------------------
header "Step 2: Verify destination (handshake)"
VERIFY_PAYLOAD=$(cat <<ENDJSON
{
  "migration_key": "$MIGRATION_KEY",
  "destination_url": "$SOURCE_INTERNAL",
  "embedding_provider": "openai",
  "embedding_model": "text-embedding-3-small",
  "embedding_dimensions": 1536
}
ENDJSON
)

# This call uses migration key auth, not admin token
VERIFY_RESP=$(curl -sf -H "Authorization: Bearer migration:$MIGRATION_KEY" \
    -H "Content-Type: application/json" \
    -X POST "$SOURCE/migration/verify-destination" -d "$VERIFY_PAYLOAD")
VERIFY_STATE=$(echo "$VERIFY_RESP" | grep -o '"state":"[^"]*"' | cut -d'"' -f4)
TOTAL_ENTRIES=$(echo "$VERIFY_RESP" | grep -o '"total_entries":[0-9]*' | cut -d: -f2)

if [ "$VERIFY_STATE" = "KEY_EXCHANGED" ] && [ "$TOTAL_ENTRIES" = "3" ]; then
    pass "Handshake complete: KEY_EXCHANGED, total_entries=$TOTAL_ENTRIES"
else
    fail "Expected KEY_EXCHANGED with 3 entries, got: $VERIFY_RESP"
    exit 1
fi

# ---------------------------------------------------------------------------
header "Step 3: Start redirect on source"
REDIRECT_PAYLOAD=$(cat <<ENDJSON
{
  "migration_key": "$MIGRATION_KEY"
}
ENDJSON
)

REDIRECT_RESP=$(curl_source -X POST "$SOURCE/migration/start-redirect" -d "$REDIRECT_PAYLOAD")
REDIRECT_STATE=$(echo "$REDIRECT_RESP" | grep -o '"state":"[^"]*"' | cut -d'"' -f4)

if [ "$REDIRECT_STATE" = "REDIRECTING" ]; then
    pass "Source is now redirecting (449)"
else
    fail "Expected REDIRECTING, got: $REDIRECT_RESP"
fi

# ---------------------------------------------------------------------------
header "Step 4: Verify ingest returns 449"
INGEST_449_CODE=$(curl -s -o /tmp/mig-449-body.json -w "%{http_code}" \
    -H "Authorization: Bearer $SOURCE_TOKEN" \
    -H "Content-Type: application/json" \
    -X POST "$SOURCE/ingest" -d "$INGEST_PAYLOAD")

if [ "$INGEST_449_CODE" = "449" ]; then
    pass "Ingest returns 449 during redirect"
else
    fail "Expected 449 but got $INGEST_449_CODE"
fi

# Check 449 body has signature
BODY_449=$(cat /tmp/mig-449-body.json)
if echo "$BODY_449" | grep -q '"signature"'; then
    pass "449 response includes Ed25519 signature"
else
    fail "Missing signature in 449 body: $BODY_449"
fi

if echo "$BODY_449" | grep -q '"redirect_url"'; then
    pass "449 response includes redirect_url"
else
    fail "Missing redirect_url in 449 body: $BODY_449"
fi

# ---------------------------------------------------------------------------
header "Step 5: Trigger transfer on destination"
HANDSHAKE_PAYLOAD=$(cat <<ENDJSON
{
  "migration_id": "$MIGRATION_ID",
  "source_url": "$SOURCE_INTERNAL",
  "migration_key": "$MIGRATION_KEY",
  "config": {}
}
ENDJSON
)

HANDSHAKE_RESP=$(curl_dest -X POST "$DEST/migration/receive/handshake" -d "$HANDSHAKE_PAYLOAD")
HANDSHAKE_STATE=$(echo "$HANDSHAKE_RESP" | grep -o '"state":"[^"]*"' | cut -d'"' -f4)

if [ "$HANDSHAKE_STATE" = "KEY_EXCHANGED" ]; then
    pass "Destination handshake accepted, transfer worker launched"
else
    fail "Expected KEY_EXCHANGED on destination, got: $HANDSHAKE_RESP"
fi

# ---------------------------------------------------------------------------
header "Step 6: Wait for transfer to complete"
ELAPSED=0
TIMEOUT=30
FINAL_STATE=""
while [ $ELAPSED -lt $TIMEOUT ]; do
    STATUS_RESP=$(curl_dest "$DEST/migration/status" 2>/dev/null || echo '{}')
    CURRENT_STATE=$(echo "$STATUS_RESP" | grep -o '"state":"[^"]*"' | cut -d'"' -f4)

    if [ "$CURRENT_STATE" = "COMPLETE" ]; then
        FINAL_STATE="COMPLETE"
        break
    elif [ "$CURRENT_STATE" = "FAILED" ]; then
        FINAL_STATE="FAILED"
        break
    fi

    sleep 1
    ELAPSED=$((ELAPSED + 1))
done

if [ "$FINAL_STATE" = "COMPLETE" ]; then
    pass "Migration completed in ${ELAPSED}s"
else
    fail "Migration did not complete within ${TIMEOUT}s (state=$CURRENT_STATE)"
    # Dump destination logs for debugging
    echo "  Destination status: $STATUS_RESP"
fi

# ---------------------------------------------------------------------------
header "Step 7: Verify data on destination"
SEARCH_RESP=$(curl_dest -X POST "$DEST/search" \
    -d '{"query":"configure memlayer production","limit":5}')
SEARCH_TOTAL=$(echo "$SEARCH_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$SEARCH_TOTAL" -ge 1 ] 2>/dev/null; then
    pass "Destination has searchable entries (total=$SEARCH_TOTAL)"
else
    fail "Expected >= 1 search result on destination, got: $SEARCH_RESP"
fi

# Verify session was transferred
SUMMARY_RESP=$(curl_dest "$DEST/sessions/$SESSION_ID/summary?limit=100")
MSG_COUNT=$(echo "$SUMMARY_RESP" | grep -o '"message_count":[0-9]*' | cut -d: -f2)

if [ "$MSG_COUNT" = "3" ]; then
    pass "Session summary has 3 messages on destination"
else
    fail "Expected 3 messages on destination, got: message_count=$MSG_COUNT"
fi

# ---------------------------------------------------------------------------
header "Step 8: Client provision"
PROVISION_RESP=$(curl -sf \
    -H "Authorization: Bearer migration:$MIGRATION_ID" \
    "$DEST/migration/client-provision")

if echo "$PROVISION_RESP" | grep -q '"auth_token"'; then
    pass "Client provision returns auth_token"
else
    fail "Client provision missing auth_token: $PROVISION_RESP"
fi

if echo "$PROVISION_RESP" | grep -q '"server_url"'; then
    pass "Client provision returns server_url"
else
    fail "Client provision missing server_url: $PROVISION_RESP"
fi

# ===========================================================================
# Error cases — need a fresh migration state, so cancel first
# ===========================================================================

header "Error: Invalid migration key"
BAD_KEY_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer migration:totally-invalid-key" \
    -H "Content-Type: application/json" \
    -X POST "$SOURCE/migration/verify-destination" \
    -d '{"migration_key":"totally-invalid-key","destination_url":"http://x"}')

if [ "$BAD_KEY_CODE" = "401" ]; then
    pass "Invalid migration key rejected (401)"
else
    fail "Expected 401 for bad key, got $BAD_KEY_CODE"
fi

# ---------------------------------------------------------------------------
header "Error: Cancel and verify recovery"

# Cancel current migration on source
CANCEL_RESP=$(curl_source -X POST "$SOURCE/migration/cancel" \
    -d "{\"migration_id\":\"$MIGRATION_ID\"}")
CANCEL_STATE=$(echo "$CANCEL_RESP" | grep -o '"state":"[^"]*"' | cut -d'"' -f4)

# Source may already be COMPLETE; cancel on destination too
curl_dest -X POST "$DEST/migration/cancel" \
    -d "{\"migration_id\":\"$MIGRATION_ID\"}" >/dev/null 2>&1 || true

if [ "$CANCEL_STATE" = "IDLE" ] || [ "$CANCEL_STATE" = "COMPLETE" ]; then
    pass "Cancel returned state=$CANCEL_STATE"
else
    fail "Expected IDLE or COMPLETE after cancel, got: $CANCEL_RESP"
fi

# Verify ingest works normally after cancel (no more 449)
INGEST_AFTER=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $SOURCE_TOKEN" \
    -H "Content-Type: application/json" \
    -X POST "$SOURCE/ingest" -d "$INGEST_PAYLOAD")

if [ "$INGEST_AFTER" = "200" ]; then
    pass "Ingest returns 200 after migration cancel (no redirect)"
else
    fail "Expected 200 after cancel, got $INGEST_AFTER"
fi

# ---------------------------------------------------------------------------
header "Error: Double initiate"

# Start a new migration
INIT2_RESP=$(curl_source -X POST "$SOURCE/migration/initiate")
INIT2_STATE=$(echo "$INIT2_RESP" | grep -o '"state":"[^"]*"' | cut -d'"' -f4)

if [ "$INIT2_STATE" = "INITIATED" ]; then
    pass "Fresh migration initiated after cancel"
else
    fail "Expected INITIATED for fresh migration, got: $INIT2_RESP"
fi

# Try to initiate again while one is active
DOUBLE_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $SOURCE_TOKEN" \
    -H "Content-Type: application/json" \
    -X POST "$SOURCE/migration/initiate")

if [ "$DOUBLE_CODE" = "409" ]; then
    pass "Double initiate rejected (409)"
else
    fail "Expected 409 for double initiate, got $DOUBLE_CODE"
fi

# Clean up the second migration
curl_source -X POST "$SOURCE/migration/cancel" -d '{}' >/dev/null 2>&1 || true
curl_dest -X POST "$DEST/migration/cancel" -d '{}' >/dev/null 2>&1 || true

# ---------------------------------------------------------------------------
header "Error: Cancel with empty body"

# Start a migration so there's something to cancel
INIT3_RESP=$(curl_source -X POST "$SOURCE/migration/initiate")
INIT3_STATE=$(echo "$INIT3_RESP" | grep -o '"state":"[^"]*"' | cut -d'"' -f4)

if [ "$INIT3_STATE" = "INITIATED" ]; then
    # Cancel with completely empty body (no -d flag, no Content-Type)
    EMPTY_CANCEL_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -H "Authorization: Bearer $SOURCE_TOKEN" \
        -X POST "$SOURCE/migration/cancel")
    if [ "$EMPTY_CANCEL_CODE" = "200" ]; then
        pass "Cancel with empty body returns 200 (not 500)"
    else
        fail "Expected 200 for cancel with empty body, got $EMPTY_CANCEL_CODE"
    fi

    # Also test with Content-Type: application/json but empty body
    INIT4_RESP=$(curl_source -X POST "$SOURCE/migration/initiate")
    EMPTY_JSON_CANCEL_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -H "Authorization: Bearer $SOURCE_TOKEN" \
        -H "Content-Type: application/json" \
        -X POST "$SOURCE/migration/cancel")
    if [ "$EMPTY_JSON_CANCEL_CODE" = "200" ]; then
        pass "Cancel with empty JSON body returns 200 (not 500)"
    else
        fail "Expected 200 for cancel with empty JSON body, got $EMPTY_JSON_CANCEL_CODE"
    fi
else
    fail "Could not initiate migration for empty-body cancel test"
fi

curl_source -X POST "$SOURCE/migration/cancel" -d '{}' >/dev/null 2>&1 || true
curl_dest -X POST "$DEST/migration/cancel" -d '{}' >/dev/null 2>&1 || true

# ===========================================================================
# Edge cases — migration with NULL fields and real data
# ===========================================================================

header "Edge: Migration preserves NULL optional fields"

# Seed source with entries that have all optional fields NULL
NULL_ENTRY_PAYLOAD=$(cat <<'ENDJSON'
{
  "entries": [{
    "payload_hash": "null-fields-001",
    "session_id": "null-fields-sess",
    "message_type": "user",
    "content_type": "user",
    "raw_content": "Entry with all optional fields null",
    "timestamp": "2026-01-20T10:00:00Z",
    "project_path": "/test/null-fields",
    "client_machine_id": "test-machine"
  }]
}
ENDJSON
)
curl_source -X POST "$SOURCE/ingest" -d "$NULL_ENTRY_PAYLOAD" > /dev/null

# Run a mini migration
NULL_INIT=$(curl_source -X POST "$SOURCE/migration/initiate")
NULL_MIG_ID=$(echo "$NULL_INIT" | grep -o '"migration_id":"[^"]*"' | cut -d'"' -f4)
NULL_MIG_KEY=$(echo "$NULL_INIT" | grep -o '"migration_key":"[^"]*"' | cut -d'"' -f4)

# Handshake
curl -sf -H "Authorization: Bearer migration:$NULL_MIG_KEY" \
    -H "Content-Type: application/json" \
    -X POST "$SOURCE/migration/verify-destination" \
    -d "{\"migration_key\":\"$NULL_MIG_KEY\",\"destination_url\":\"$SOURCE_INTERNAL\",\"embedding_provider\":\"openai\",\"embedding_model\":\"text-embedding-3-small\",\"embedding_dimensions\":1536}" > /dev/null

# Start redirect + trigger transfer
curl_source -X POST "$SOURCE/migration/start-redirect" \
    -d "{\"migration_key\":\"$NULL_MIG_KEY\"}" > /dev/null

curl_dest -X POST "$DEST/migration/receive/handshake" \
    -d "{\"migration_id\":\"$NULL_MIG_ID\",\"source_url\":\"$SOURCE_INTERNAL\",\"migration_key\":\"$NULL_MIG_KEY\",\"config\":{}}" > /dev/null

# Wait for completion
NULL_ELAPSED=0
NULL_DONE=""
while [ $NULL_ELAPSED -lt 30 ]; do
    NULL_STATUS=$(curl_dest "$DEST/migration/status" 2>/dev/null || echo '{}')
    NULL_STATE=$(echo "$NULL_STATUS" | grep -o '"state":"[^"]*"' | cut -d'"' -f4)
    if [ "$NULL_STATE" = "COMPLETE" ] || [ "$NULL_STATE" = "FAILED" ]; then
        NULL_DONE="$NULL_STATE"
        break
    fi
    sleep 1
    NULL_ELAPSED=$((NULL_ELAPSED + 1))
done

if [ "$NULL_DONE" = "COMPLETE" ]; then
    pass "NULL-fields migration completed"
else
    fail "NULL-fields migration did not complete (state=$NULL_STATE)"
fi

# Verify the null-fields entry exists on destination
NULL_SESS=$(curl_dest "$DEST/sessions/null-fields-sess/summary?limit=10" 2>/dev/null || echo '{}')
NULL_MSG_COUNT=$(echo "$NULL_SESS" | grep -o '"message_count":[0-9]*' | cut -d: -f2)

if [ "$NULL_MSG_COUNT" = "1" ]; then
    pass "NULL-fields entry migrated to destination"
else
    fail "Expected 1 message in null-fields session, got: $NULL_MSG_COUNT"
fi

# Clean up migrations
curl_source -X POST "$SOURCE/migration/cancel" -d '{}' >/dev/null 2>&1 || true
curl_dest -X POST "$DEST/migration/cancel" -d '{}' >/dev/null 2>&1 || true

# ---------------------------------------------------------------------------
header "Results"
echo ""
printf "  \033[32mPassed: %d\033[0m\n" "$PASSED"
printf "  \033[31mFailed: %d\033[0m\n" "$FAILED"
echo ""

if [ "$FAILED" -gt 0 ]; then
    exit 1
fi
echo "All migration E2E tests passed!"
