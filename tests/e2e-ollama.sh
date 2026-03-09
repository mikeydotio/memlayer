#!/usr/bin/env bash
# SLOW E2E test: Ollama embedding integration
# Requires: docker-compose.test.yml with --profile ollama
#
# Usage:
#   docker compose -f docker-compose.test.yml --profile ollama up -d --build --wait
#   docker exec memlayer-test-ollama ollama pull nomic-embed-text
#   ./tests/e2e-ollama.sh
#   docker compose -f docker-compose.test.yml --profile ollama down -v
set -euo pipefail

API="http://127.0.0.1:8422/api"
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
header "Health check (Ollama server)"
HEALTH=$(curl -sf http://127.0.0.1:8422/health)
if echo "$HEALTH" | grep -q '"status":"ok"'; then
    pass "Ollama server is healthy"
else
    fail "Health check failed: $HEALTH"
    exit 1
fi

# Check that Ollama provider is configured
if echo "$HEALTH" | grep -q '"embeddings":"ok (ollama)"'; then
    pass "Embedding provider is Ollama"
else
    fail "Expected Ollama provider: $HEALTH"
fi

# ---------------------------------------------------------------------------
header "Ingest entries for embedding"
SESSION_ID="ollama-test-$(date +%s)"
NOW=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

INGEST_PAYLOAD=$(cat <<ENDJSON
{
  "entries": [
    {
      "payload_hash": "ollama-hash-001",
      "session_id": "$SESSION_ID",
      "message_type": "user",
      "content_type": "user",
      "raw_content": "What is the capital of France?",
      "timestamp": "$NOW",
      "project_path": "/home/e2e/ollama-test",
      "client_machine_id": "e2e-machine"
    },
    {
      "payload_hash": "ollama-hash-002",
      "session_id": "$SESSION_ID",
      "message_type": "assistant",
      "content_type": "assistant",
      "raw_content": "The capital of France is Paris. It is the largest city in France and serves as the country's political and cultural center.",
      "timestamp": "$NOW",
      "project_path": "/home/e2e/ollama-test",
      "client_machine_id": "e2e-machine"
    }
  ]
}
ENDJSON
)

INGEST_RESP=$(curl_api -X POST "$API/ingest" -d "$INGEST_PAYLOAD")
ACCEPTED=$(echo "$INGEST_RESP" | grep -o '"accepted":[0-9]*' | cut -d: -f2)

if [ "$ACCEPTED" = "2" ]; then
    pass "Ingested 2 entries"
else
    fail "Expected 2 accepted, got: $INGEST_RESP"
fi

# ---------------------------------------------------------------------------
header "Wait for embedding worker to process"
MAX_WAIT=60
ELAPSED=0
while [ $ELAPSED -lt $MAX_WAIT ]; do
    STATUS=$(curl_api "$API/embeddings/status")
    EMBEDDED=$(echo "$STATUS" | grep -o '"embedded":[0-9]*' | cut -d: -f2)
    if [ "$EMBEDDED" = "2" ]; then
        break
    fi
    sleep 3
    ELAPSED=$((ELAPSED + 3))
done

if [ "$EMBEDDED" = "2" ]; then
    pass "Both entries embedded (took ~${ELAPSED}s)"
else
    fail "Embedding timed out after ${MAX_WAIT}s. Status: $STATUS"
fi

# ---------------------------------------------------------------------------
header "Embedding status endpoint"
STATUS=$(curl_api "$API/embeddings/status")
ENABLED=$(echo "$STATUS" | grep -o '"enabled":true')
PROVIDER=$(echo "$STATUS" | grep -o '"provider":"ollama"')

if [ -n "$ENABLED" ] && [ -n "$PROVIDER" ]; then
    pass "Embedding status shows enabled=true, provider=ollama"
else
    fail "Unexpected status: $STATUS"
fi

# ---------------------------------------------------------------------------
header "Hybrid search (FTS + vector)"
SEARCH_RESP=$(curl_api -X POST "$API/search" \
    -d '{"query":"capital of France","limit":5}')
TOTAL=$(echo "$SEARCH_RESP" | grep -o '"total":[0-9]*' | cut -d: -f2)

if [ "$TOTAL" -ge 1 ]; then
    pass "Hybrid search returned $TOTAL results"
else
    fail "Expected >= 1 result: $SEARCH_RESP"
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
echo "All Ollama integration tests passed!"
