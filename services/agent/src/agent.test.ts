import { describe, test, expect, beforeEach } from "bun:test";
import { localRulesEngine, resetEscalationState } from "./agent";
import type { OperationalSnapshot } from "./types";

function makeSnapshot(
  overrides: Partial<OperationalSnapshot> = {},
): OperationalSnapshot {
  return {
    currentSlot: 100000,
    latestFinalizedSlot: 99970,
    tipMedianLamports: 1000,
    recentOutcomes: [],
    activeBundleCount: 0,
    snapshotAt: Date.now(),
    totalRetries: 0,
    totalRetriesSucceeded: 0,
    totalRetriesExhausted: 0,
    ...overrides,
  };
}

describe("Local Rules Engine", () => {
  beforeEach(() => {
    resetEscalationState();
  });

  test("no failures → should not retry", () => {
    const snapshot = makeSnapshot({
      recentOutcomes: [
        {
          bundleId: "b1",
          slot: 100,
          stage: "Finalized",
          failureReason: "",
          failureStage: "",
          recovery: "",
          tipLamports: 1000,
          blockhash: "abc",
          submittedAt: 100,
          originalBundleId: "",
          retryAttempt: 0,
        },
      ],
    });

    const decision = localRulesEngine(snapshot);
    expect(decision.shouldRetry).toBe(false);
    expect(decision.confidence).toBe(1.0);
    expect(decision.failureClassification).toBe("None");
  });

  test("expired blockhash → should retry with refresh", () => {
    const snapshot = makeSnapshot({
      recentOutcomes: [
        {
          bundleId: "b1",
          slot: 100,
          stage: "Failed",
          failureReason: "expired_blockhash",
          failureStage: "execution",
          recovery: "Fetch fresh blockhash",
          tipLamports: 1000,
          blockhash: "abc",
          submittedAt: 100,
          originalBundleId: "",
          retryAttempt: 0,
        },
      ],
    });

    const decision = localRulesEngine(snapshot);
    expect(decision.shouldRetry).toBe(true);
    expect(decision.refreshBlockhash).toBe(true);
    expect(decision.failureClassification).toBe("expired_blockhash");
    expect(decision.suggestedTipLamports).toBeGreaterThan(1000);
  });

  test("fee too low → should retry with higher tip", () => {
    const snapshot = makeSnapshot({
      tipMedianLamports: 2000,
      recentOutcomes: [
        {
          bundleId: "b1",
          slot: 100,
          stage: "Failed",
          failureReason: "fee too low",
          failureStage: "submission",
          recovery: "Increase tip",
          tipLamports: 1000,
          blockhash: "abc",
          submittedAt: 100,
          originalBundleId: "",
          retryAttempt: 0,
        },
      ],
    });

    const decision = localRulesEngine(snapshot);
    expect(decision.shouldRetry).toBe(true);
    expect(decision.failureClassification).toBe("fee_too_low");
    expect(decision.suggestedTipLamports).toBe(3000); // 2000 * 1.5
  });

  test("non-recoverable failure → should not retry", () => {
    const snapshot = makeSnapshot({
      recentOutcomes: [
        {
          bundleId: "b1",
          slot: 100,
          stage: "Failed",
          failureReason: "compute_exceeded",
          failureStage: "execution",
          recovery: "Reduce compute usage",
          tipLamports: 1000,
          blockhash: "abc",
          submittedAt: 100,
          originalBundleId: "",
          retryAttempt: 0,
        },
      ],
    });

    const decision = localRulesEngine(snapshot);
    expect(decision.shouldRetry).toBe(false);
    expect(decision.confidence).toBeGreaterThanOrEqual(0.9);
  });

  test("critical failure rate → should hold", () => {
    const outcomes = [];
    for (let i = 0; i < 5; i++) {
      outcomes.push({
        bundleId: `b${i}`,
        slot: 100 + i,
        stage: i < 4 ? "Failed" : "Finalized",
        failureReason: i < 4 ? "bundle_failure" : "",
        failureStage: i < 4 ? "execution" : "",
        recovery: "",
        tipLamports: 1000,
        blockhash: "abc",
        submittedAt: 100,
        originalBundleId: "",
        retryAttempt: 0,
      });
    }

    const snapshot = makeSnapshot({ recentOutcomes: outcomes });
    const decision = localRulesEngine(snapshot);
    expect(decision.shouldRetry).toBe(false);
    expect(decision.confidence).toBeGreaterThanOrEqual(0.8);
  });

  test("consecutive retry failures → enters hold mode", () => {
    const retryFailures = [];
    for (let i = 0; i < 3; i++) {
      retryFailures.push({
        bundleId: `retry-${i}`,
        slot: 100 + i,
        stage: "Failed",
        failureReason: "bundle_failure",
        failureStage: "execution",
        recovery: "",
        tipLamports: 1000,
        blockhash: "abc",
        submittedAt: 100,
        originalBundleId: "original-bundle",
        retryAttempt: i + 1,
      });
    }

    const snapshot = makeSnapshot({ recentOutcomes: retryFailures });
    const decision = localRulesEngine(snapshot);
    expect(decision.shouldRetry).toBe(false);
    expect(decision.failureClassification).toBe("escalation_hold");

    // Subsequent call should still be in hold mode
    const decision2 = localRulesEngine(snapshot);
    expect(decision2.shouldRetry).toBe(false);
    expect(decision2.failureClassification).toBe("hold_mode");
  });
});
