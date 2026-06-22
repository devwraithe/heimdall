export interface BundleOutcomeSummary {
  bundleId: string;
  slot: number;
  stage: string;
  failureReason: string;
  tipLamports: number;
  blockhash: string;
  submittedAt: number;
}

export interface OperationalSnapshot {
  currentSlot: number;
  latestFinalizedSlot: number;
  tipMedianLamports: number;
  recentOutcomes: BundleOutcomeSummary[];
  activeBundleCount: number;
  snapshotAt: number;
}
