import { createL5Stream } from "./client";
import { makeRetryDecision } from "./agent";
import type { OperationalSnapshot } from "./types";

const DECISION_INTERVAL_MS = 30000; // from 2s to 30s
let lastDecisionTime = 0;

async function handleSnapshot(snapshot: OperationalSnapshot): Promise<void> {
  const now = Date.now();

  console.log(
    `Snapshot received: slot=${snapshot.currentSlot} tip=${snapshot.tipMedianLamports}`,
  );

  // Rate limit agent calls — once every 2 seconds
  if (now - lastDecisionTime < DECISION_INTERVAL_MS) {
    return;
  }

  lastDecisionTime = now;

  try {
    const decision = await makeRetryDecision(snapshot);

    if (decision.shouldRetry) {
      console.log(`
=== AGENT RETRY DECISION ===
Reason: ${decision.reason}
Failure: ${decision.failureClassification}
Refresh blockhash: ${decision.refreshBlockhash}
Suggested tip: ${decision.suggestedTipLamports} lamports
============================
      `);
    }
  } catch (err: any) {
    if (err?.status === 429) {
      console.log("Rate limited — waiting 30s before next decision");
      lastDecisionTime = now + 28000;
    } else {
      console.error("Agent decision failed:", err);
    }
  }
}

function main(): void {
  console.log("Heimdall L6 AI Agent starting...");
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
