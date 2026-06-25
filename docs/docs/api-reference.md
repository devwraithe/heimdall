---
sidebar_position: 8
title: API Reference
---

# Monitoring API Reference

L7 exposes an HTTP API at `http://localhost:3000` (configurable).

## Endpoints

### `GET /health`

System health and uptime.

**Response:**
```json
{
  "status": "ok",
  "uptime_seconds": 142,
  "started_at": "2026-06-23T14:00:00.000Z"
}
```

### `GET /metrics`

Live network metrics and bundle statistics.

**Response:**
```json
{
  "current_slot": 428015020,
  "latest_finalized_slot": 428014988,
  "tip_median_lamports": 1065803,
  "active_bundle_count": 0,
  "total_bundles_submitted": 12,
  "total_bundles_failed": 2,
  "total_bundles_finalized": 10,
  "total_retries": 3,
  "retries_succeeded": 2,
  "retries_exhausted": 1,
  "uptime_seconds": 42
}
```

### `GET /outcomes`

Recent bundle outcomes with three-field failure classification and retry lineage.

**Response:**
```json
{
  "outcomes": [
    {
      "bundleId": "562e2333...",
      "slot": 428136003,
      "stage": "Finalized",
      "failureReason": "",
      "failureStage": "",
      "recovery": "",
      "tipLamports": 1515296,
      "originalBundleId": "",
      "retryAttempt": 0
    }
  ]
}
```

### `GET /decisions`

Recent AI agent decisions with confidence and risk assessment.

**Response:**
```json
{
  "decisions": [
    {
      "timestamp": 1719158400000,
      "shouldRetry": true,
      "reason": "1 bundle failed due to expired blockhash...",
      "failureClassification": "expired_blockhash",
      "refreshBlockhash": true,
      "suggestedTipLamports": 1300,
      "confidence": 0.9,
      "observedRisk": "Blockhash expiry indicates...",
      "source": "gemini"
    }
  ]
}
```

### `POST /decisions`

Record an agent decision (called by L6).

**Request:**
```json
{
  "timestamp": 1719158400000,
  "shouldRetry": true,
  "reason": "...",
  "failureClassification": "expired_blockhash",
  "refreshBlockhash": true,
  "suggestedTipLamports": 1300,
  "confidence": 0.9,
  "observedRisk": "...",
  "source": "gemini"
}
```

**Response:** `{ "accepted": true }`

### `GET /events`

Server-Sent Events (SSE) stream. Connect with `EventSource` or `curl`.

Each event contains the full SSE data payload with current slot, outcomes, decisions, and metrics.

```bash
curl http://localhost:3000/events
```

### `GET /evidence`

Download a Markdown evidence report suitable for judge submission.

```bash
curl -o evidence.md http://localhost:3000/evidence
```

The report includes:
- Network metrics table (slot, tip, uptime, retry metrics)
- Recent bundle outcomes with retry lineage
- AI agent decision history with confidence and risk

## gRPC Service (L5)

L5 exposes `OperationalStateService` on port `50051`:

```protobuf
service OperationalStateService {
  rpc Subscribe(SubscribeRequest) returns (stream OperationalSnapshot);
  rpc Retry(RetryRequest) returns (RetryResponse);
  rpc Health(HealthRequest) returns (HealthResponse);
}
```
