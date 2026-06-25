import { GoogleGenAI } from "@google/genai";
import type { OperationalSnapshot, RetryDecision } from "./types";

import dotenv from "dotenv";
import path from "path";
dotenv.config({ path: path.resolve("../../.env") });

const ai = new GoogleGenAI({ apiKey: process.env.GEMINI_API_KEY! });

// ──────────────────────────────────────────────
//  Failure escalation state
// ──────────────────────────────────────────────

let consecutiveFailures = 0;
let holdModeUntil = 0;

const HOLD_MODE_THRESHOLD = 3; // Enter hold after 3 consecutive failures
const HOLD_MODE_DURATION_MS = 60_000; // Hold for 60 seconds

const NON_RECOVERABLE_TYPES = [
  "compute_exceeded",
  "program_error",
  "account_not_found",
];

// ──────────────────────────────────────────────
//  Tier 1: Local deterministic rules engine
// ──────────────────────────────────────────────

export function localRulesEngine(snapshot: OperationalSnapshot): RetryDecision {
  const failedBundles = snapshot.recentOutcomes.filter(
    (o) => o.stage === "Failed" || o.failureReason !== "",
  );
  const totalRuns = snapshot.recentOutcomes.length;
  const landedRate =
    totalRuns > 0
      ? snapshot.recentOutcomes.filter(
          (o) => o.stage === "Finalized" || o.stage === "Confirmed",
        ).length / totalRuns
      : 1.0;

  const slotGap = snapshot.currentSlot - snapshot.latestFinalizedSlot;
  const baseTip = snapshot.tipMedianLamports || 1_000;

  // ── Check hold mode ──
  if (Date.now() < holdModeUntil) {
    const remainingSecs = Math.ceil((holdModeUntil - Date.now()) / 1000);
    return {
      shouldRetry: false,
      reason: `HOLD MODE active. ${remainingSecs}s remaining. System is holding all retries due to ${consecutiveFailures} consecutive failures. Waiting for network stabilization.`,
      refreshBlockhash: false,
      suggestedTipLamports: baseTip,
      failureClassification: "hold_mode",
      confidence: 1.0,
      observedRisk: `Escalation triggered after ${HOLD_MODE_THRESHOLD} consecutive failures. Network may be degraded or bundles may be non-viable under current conditions.`,
    };
  }

  // ── No failures — reset escalation state ──
  if (failedBundles.length === 0) {
    consecutiveFailures = 0;
    return {
      shouldRetry: false,
      reason: "No failed bundles detected in recent outcomes.",
      refreshBlockhash: false,
      suggestedTipLamports: baseTip,
      failureClassification: "None",
      confidence: 1.0,
      observedRisk: "No risk detected. All recent bundles landed successfully.",
    };
  }

  // ── Check for non-recoverable failures ──
  const nonRecoverableFailures = failedBundles.filter((b) =>
    NON_RECOVERABLE_TYPES.some((t) =>
      b.failureReason.toLowerCase().includes(t),
    ),
  );
  if (nonRecoverableFailures.length > 0) {
    return {
      shouldRetry: false,
      reason: `${nonRecoverableFailures.length} bundle(s) failed with non-recoverable errors (${nonRecoverableFailures[0]?.failureReason}). Retrying will not help — the transaction itself must be fixed.`,
      refreshBlockhash: false,
      suggestedTipLamports: baseTip,
      failureClassification:
        nonRecoverableFailures[0]?.failureReason || "non_recoverable",
      confidence: 0.95,
      observedRisk: `Non-recoverable failure detected. Recovery: ${nonRecoverableFailures[0]?.recovery || "Fix the transaction payload and resubmit."}`,
    };
  }

  // ── Escalation check — enter hold mode if too many consecutive failures ──
  const recentRetries = snapshot.recentOutcomes.filter(
    (o) => o.retryAttempt > 0 && o.stage === "Failed",
  );
  if (recentRetries.length >= HOLD_MODE_THRESHOLD) {
    consecutiveFailures = recentRetries.length;
    holdModeUntil = Date.now() + HOLD_MODE_DURATION_MS;
    return {
      shouldRetry: false,
      reason: `ENTERING HOLD MODE. ${recentRetries.length} consecutive retry failures detected. All retries suspended for ${HOLD_MODE_DURATION_MS / 1000}s to allow network recovery.`,
      refreshBlockhash: false,
      suggestedTipLamports: baseTip,
      failureClassification: "escalation_hold",
      confidence: 0.9,
      observedRisk: `Repeated retry failures indicate a systemic issue (network congestion, leader skipping, or persistent tip underbidding). Hold mode prevents resource waste.`,
    };
  }

  // ── Classify the dominant failure type ──
  const failureTypes = failedBundles.map((b) => b.failureReason);
  const hasExpiredBlockhash = failureTypes.some(
    (f) =>
      f.toLowerCase().includes("expired") ||
      f.toLowerCase().includes("blockhash"),
  );
  const hasFeeTooLow = failureTypes.some(
    (f) => f.toLowerCase().includes("fee") && f.toLowerCase().includes("low"),
  );

  // ── High failure rate with few runs — hold ──
  if (landedRate < 0.3 && totalRuns >= 5) {
    consecutiveFailures++;
    return {
      shouldRetry: false,
      reason: `Landed rate critically low (${(landedRate * 100).toFixed(0)}%) across ${totalRuns} runs. Holding submissions until network stabilizes.`,
      refreshBlockhash: false,
      suggestedTipLamports: baseTip,
      failureClassification: failedBundles[0]?.failureReason || "Unknown",
      confidence: 0.85,
      observedRisk: `Slot gap: ${slotGap}. Network may be under stress. ${failedBundles.length} recent failures detected.`,
    };
  }

  // ── Expired blockhash — retry with fresh blockhash ──
  if (hasExpiredBlockhash) {
    consecutiveFailures = 0;
    const tipPremium = Math.round(baseTip * 1.3);
    return {
      shouldRetry: true,
      reason: `${failedBundles.length} bundle(s) failed due to expired blockhash. Retrying with fresh blockhash and +30% tip premium.`,
      refreshBlockhash: true,
      suggestedTipLamports: tipPremium,
      failureClassification: "expired_blockhash",
      confidence: 0.9,
      observedRisk: `Blockhash expiry indicates the bundle exceeded the 150-slot validity window. Slot gap: ${slotGap}.`,
    };
  }

  // ── Fee too low — retry with higher tip ──
  if (hasFeeTooLow) {
    consecutiveFailures = 0;
    const tipPremium = Math.round(baseTip * 1.5);
    return {
      shouldRetry: true,
      reason: `Bundle rejected due to insufficient tip. Retrying with +50% tip premium.`,
      refreshBlockhash: true,
      suggestedTipLamports: tipPremium,
      failureClassification: "fee_too_low",
      confidence: 0.85,
      observedRisk: `Tip competition may be elevated. Current median: ${baseTip} lamports.`,
    };
  }

  // ── Moderate failure rate — retry with congestion premium ──
  if (landedRate < 0.5 && totalRuns >= 3) {
    const tipPremium = Math.round(baseTip * 1.3);
    return {
      shouldRetry: true,
      reason: `Landed rate at ${(landedRate * 100).toFixed(0)}% with recoverable failures. Retrying with fresh blockhash and congestion premium.`,
      refreshBlockhash: true,
      suggestedTipLamports: tipPremium,
      failureClassification: failedBundles[0]?.failureReason || "Unknown",
      confidence: 0.75,
      observedRisk: `Slot gap: ${slotGap}. ${failedBundles.length} failures in last ${totalRuns} runs. Network conditions uncertain.`,
    };
  }

  // ── Default: retry with standard tip ──
  return {
    shouldRetry: true,
    reason: `${failedBundles.length} failed bundle(s) detected. Retrying with fresh blockhash.`,
    refreshBlockhash: true,
    suggestedTipLamports: baseTip,
    failureClassification: failedBundles[0]?.failureReason || "Unknown",
    confidence: 0.7,
    observedRisk: `Slot gap: ${slotGap}. Failure cause may require investigation.`,
  };
}

// ──────────────────────────────────────────────
//  Tier 2: Gemini LLM reasoning
// ──────────────────────────────────────────────

async function geminiReasoning(
  snapshot: OperationalSnapshot,
  localDecision: RetryDecision,
): Promise<RetryDecision> {
  const failedBundles = snapshot.recentOutcomes.filter(
    (o) => o.stage === "Failed" || o.failureReason !== "",
  );

  const prompt = `
You are an autonomous Solana transaction retry agent for a Jito bundle submission system.
You are the second tier in a two-tier decision pipeline. A local rules engine has already
produced a preliminary decision. You must reason independently and may override or refine it.

Current network state:
- Current slot: ${snapshot.currentSlot}
- Latest finalized slot: ${snapshot.latestFinalizedSlot}
- Slot gap (processing lag): ${snapshot.currentSlot - snapshot.latestFinalizedSlot}
- Active bundles in flight: ${snapshot.activeBundleCount}
- Current tip median: ${snapshot.tipMedianLamports} lamports
- Total retries attempted: ${snapshot.totalRetries}
- Retries succeeded: ${snapshot.totalRetriesSucceeded}
- Retries exhausted (all 4 attempts failed): ${snapshot.totalRetriesExhausted}

Failed bundles (${failedBundles.length}):
${failedBundles
  .map(
    (b) => `
  - Bundle ID: ${b.bundleId}
  - Target slot: ${b.slot}
  - Failure type: ${b.failureReason || "Unknown"}
  - Failure stage: ${b.failureStage || "Unknown"}
  - Recovery guidance: ${b.recovery || "None"}
  - Tip paid: ${b.tipLamports} lamports
  - Blockhash: ${b.blockhash}
  - Original bundle: ${b.originalBundleId || "N/A (first submission)"}
  - Retry attempt: ${b.retryAttempt}
`,
  )
  .join("")}

Local rules engine decision:
- Should retry: ${localDecision.shouldRetry}
- Reason: ${localDecision.reason}
- Refresh blockhash: ${localDecision.refreshBlockhash}
- Suggested tip: ${localDecision.suggestedTipLamports} lamports
- Failure classification: ${localDecision.failureClassification}
- Confidence: ${localDecision.confidence}
- Observed risk: ${localDecision.observedRisk}

Hard constraints:
- Never recommend a tip below ${Math.max(snapshot.tipMedianLamports, 1000)} lamports (current median floor)
- Never recommend a tip above 100000 lamports (budget cap)
- If the failure is an expired blockhash, you MUST set refreshBlockhash to true
- Non-recoverable failures (compute_exceeded, program_error, account_not_found) should NOT be retried

Reason through this and respond ONLY with a valid JSON object:
{
  "shouldRetry": true or false,
  "reason": "your independent reasoning",
  "refreshBlockhash": true or false,
  "suggestedTipLamports": <number within constraints>,
  "failureClassification": "expired_blockhash" | "fee_too_low" | "bundle_failure" | "compute_exceeded" | "unknown",
  "confidence": <0.0 to 1.0>,
  "observedRisk": "plain-english risk assessment"
}`;

  const response = await ai.models.generateContent({
    model: "gemini-2.5-flash",
    contents: prompt,
  });

  const text = response.text ?? "";

  console.log("🤖 Gemini Response:\n");
  console.log(text);

  const jsonMatch = text.match(/\{[\s\S]*\}/);
  if (!jsonMatch) {
    throw new Error(`Agent returned non-JSON response: ${text}`);
  }

  const decision: RetryDecision = JSON.parse(jsonMatch[0]);

  // Enforce hard constraints on LLM output
  decision.suggestedTipLamports = Math.max(
    decision.suggestedTipLamports,
    Math.max(snapshot.tipMedianLamports, 1000),
  );
  decision.suggestedTipLamports = Math.min(
    decision.suggestedTipLamports,
    100_000,
  );

  return decision;
}

// ──────────────────────────────────────────────
//  Public API: Two-tier decision pipeline
// ──────────────────────────────────────────────

export async function makeRetryDecision(
  snapshot: OperationalSnapshot,
): Promise<RetryDecision & { source: "local_rules" | "gemini" | "fallback" }> {
  // Tier 1: Local deterministic rules
  const localDecision = localRulesEngine(snapshot);

  // If local rules say don't retry (hold mode, no failures, non-recoverable), skip Gemini
  if (!localDecision.shouldRetry) {
    return { ...localDecision, source: "local_rules" };
  }

  // Tier 2: Gemini LLM reasoning (with fallback)
  try {
    const geminiDecision = await geminiReasoning(snapshot, localDecision);
    return { ...geminiDecision, source: "gemini" };
  } catch (err: any) {
    console.warn(
      "⚠️  Gemini reasoning failed, using local rules as fallback:",
      err.message || err,
    );
    return { ...localDecision, source: "fallback" };
  }
}

// ──────────────────────────────────────────────
//  Exported for testing
// ──────────────────────────────────────────────
export function resetEscalationState(): void {
  consecutiveFailures = 0;
  holdModeUntil = 0;
}
