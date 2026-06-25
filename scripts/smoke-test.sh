#!/bin/bash
# ──────────────────────────────────────────────
# Heimdall Smoke Test
# Validates the full pipeline is operational.
# Run this AFTER starting all 3 services.
# ──────────────────────────────────────────────

set -e

MONITOR_URL="${MONITOR_URL:-http://localhost:3000}"
PASS=0
FAIL=0
WARN=0

green() { echo -e "\033[32m✓ $1\033[0m"; PASS=$((PASS + 1)); }
red()   { echo -e "\033[31m✗ $1\033[0m"; FAIL=$((FAIL + 1)); }
yellow(){ echo -e "\033[33m⚠ $1\033[0m"; WARN=$((WARN + 1)); }

echo ""
echo "⚡ Heimdall Smoke Test"
echo "───────────────────────────────────────────"

# 1. L7 health check
echo ""
echo "▸ Service Health"
if curl -sf "${MONITOR_URL}/health" > /dev/null 2>&1; then
  green "L7 monitoring is healthy"
else
  red "L7 monitoring unreachable at ${MONITOR_URL}/health"
  echo "  Make sure all services are running. Aborting."
  exit 1
fi

# 2. Metrics endpoint returns data
echo ""
echo "▸ Metrics Endpoint"
METRICS=$(curl -sf "${MONITOR_URL}/metrics" 2>/dev/null || echo "{}")
SLOT=$(echo "$METRICS" | grep -o '"current_slot":[0-9]*' | head -1 | cut -d: -f2)
if [ -n "$SLOT" ] && [ "$SLOT" -gt 0 ] 2>/dev/null; then
  green "Metrics endpoint returns slot data (slot: $SLOT)"
else
  yellow "Metrics endpoint returned no slot data (L5 gRPC may not be connected)"
fi

TIP=$(echo "$METRICS" | grep -o '"tip_median_lamports":[0-9]*' | head -1 | cut -d: -f2)
if [ -n "$TIP" ] && [ "$TIP" -gt 0 ] 2>/dev/null; then
  green "Tip median is non-zero ($TIP lamports)"
else
  yellow "Tip median is zero (no bundles submitted yet)"
fi

# 3. Outcomes endpoint
echo ""
echo "▸ Outcomes Endpoint"
OUTCOMES=$(curl -sf "${MONITOR_URL}/outcomes" 2>/dev/null || echo "{}")
OUTCOME_COUNT=$(echo "$OUTCOMES" | grep -o '"bundleId"' | wc -l | tr -d ' ')
if [ "$OUTCOME_COUNT" -gt 0 ]; then
  green "Outcomes endpoint has $OUTCOME_COUNT bundle(s)"
else
  yellow "Outcomes endpoint has no bundles (expected if no submissions yet)"
fi

# 4. Decisions endpoint
echo ""
echo "▸ Decisions Endpoint"
DECISIONS=$(curl -sf "${MONITOR_URL}/decisions" 2>/dev/null || echo "{}")
DECISION_COUNT=$(echo "$DECISIONS" | grep -o '"shouldRetry"' | wc -l | tr -d ' ')
if [ "$DECISION_COUNT" -gt 0 ]; then
  green "Decisions endpoint has $DECISION_COUNT decision(s)"
else
  yellow "Decisions endpoint has no decisions (expected if agent hasn't acted yet)"
fi

# 5. SSE stream test
echo ""
echo "▸ SSE Stream"
SSE_DATA=$(timeout 3 curl -sf "${MONITOR_URL}/events" 2>/dev/null | head -1 || echo "")
if echo "$SSE_DATA" | grep -q "currentSlot"; then
  green "SSE stream is active and producing data"
else
  yellow "SSE stream returned no data within 3s timeout"
fi

# 6. Evidence export
echo ""
echo "▸ Evidence Export"
EVIDENCE=$(curl -sf "${MONITOR_URL}/evidence" 2>/dev/null || echo "")
if echo "$EVIDENCE" | grep -q "Heimdall"; then
  LINES=$(echo "$EVIDENCE" | wc -l | tr -d ' ')
  green "Evidence endpoint returns Markdown report ($LINES lines)"
else
  red "Evidence endpoint failed or returned empty"
fi

# 7. POST decisions (integration test)
echo ""
echo "▸ Decision POST"
POST_RESULT=$(curl -sf -X POST "${MONITOR_URL}/decisions" \
  -H "Content-Type: application/json" \
  -d '{
    "timestamp": '"$(date +%s000)"',
    "shouldRetry": false,
    "reason": "smoke test",
    "failureClassification": "test",
    "refreshBlockhash": false,
    "suggestedTipLamports": 1000,
    "confidence": 1.0,
    "observedRisk": "none",
    "source": "local_rules"
  }' 2>/dev/null || echo '{"accepted": false}')
if echo "$POST_RESULT" | grep -q '"accepted":true'; then
  green "Decision POST accepted"
else
  red "Decision POST rejected"
fi

# 8. Check lifecycle log exists
echo ""
echo "▸ Lifecycle Log"
if [ -f "core/lifecycle.log" ]; then
  LOG_LINES=$(wc -l < core/lifecycle.log | tr -d ' ')
  green "lifecycle.log exists ($LOG_LINES entries)"
elif [ -f "core/final_lifecycle.log" ]; then
  LOG_LINES=$(wc -l < core/final_lifecycle.log | tr -d ' ')
  green "final_lifecycle.log exists ($LOG_LINES entries)"
else
  yellow "No lifecycle log found (expected if no bundles submitted)"
fi

# Summary
echo ""
echo "───────────────────────────────────────────"
echo "  Results: ${PASS} passed, ${FAIL} failed, ${WARN} warnings"
echo ""

if [ "$FAIL" -gt 0 ]; then
  echo -e "\033[31m  SMOKE TEST FAILED\033[0m"
  exit 1
else
  echo -e "\033[32m  SMOKE TEST PASSED\033[0m"
  exit 0
fi
