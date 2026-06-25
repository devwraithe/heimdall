import { createL5Stream, recordDecision, sendRetryDecision } from "./client";
import { makeRetryDecision } from "./agent";
import type { OperationalSnapshot } from "./types";

const DECISION_INTERVAL_MS = 30_000;
let lastDecisionTime = 0;

async function handleSnapshot(snapshot: OperationalSnapshot): Promise<void> {
  const now = Date.now();

  console.log(
    `Snapshot received: slot=${snapshot.currentSlot} tip=${snapshot.tipMedianLamports}`,
  );

  // Rate limit agent calls — once every 30 seconds
  if (now - lastDecisionTime < DECISION_INTERVAL_MS) {
    return;
  }

  lastDecisionTime = now;

  try {
    const decision = await makeRetryDecision(snapshot);
    await recordDecision({ timestamp: Date.now(), ...decision });

    console.log(`
      === AGENT DECISION (${decision.source.toUpperCase()}) ===
      Action: ${decision.shouldRetry ? "RETRY" : "HOLD"}
      Reason: ${decision.reason}
      Failure: ${decision.failureClassification}
      Refresh blockhash: ${decision.refreshBlockhash}
      Suggested tip: ${decision.suggestedTipLamports} lamports
      Confidence: ${(decision.confidence * 100).toFixed(0)}%
      Risk: ${decision.observedRisk}
      Source: ${decision.source}
      ==========================================
    `);

    if (decision.shouldRetry) {
      const failedBundles = snapshot.recentOutcomes.filter(
        (o) => o.stage === "Failed" || o.failureReason !== "",
      );

      await sendRetryDecision({
        failedBundles,
        refreshBlockhash: decision.refreshBlockhash,
        suggestedTipLamports: decision.suggestedTipLamports,
        failureClassification: decision.failureClassification,
        confidence: decision.confidence,
        observedRisk: decision.observedRisk,
      });
    }
  } catch (err: any) {
    if (err?.status === 429) {
      console.log("Rate limited — waiting 30s before next decision");
      lastDecisionTime = now + 28_000;
    } else {
      console.error("Agent decision failed:", err);
    }
  }
}

function main(): void {
  console.log("Heimdall L6 AI Agent starting...");
  console.log("Two-tier pipeline: Local Rules → Gemini LLM (with fallback)");
  console.log("Connecting to L5 at localhost:50051");

  createL5Stream(
    (snapshot) => {
      handleSnapshot(snapshot).catch(console.error);
    },
    (err) => {
      console.error("L5 stream error:", err.message);
    },
  );
}

main();
