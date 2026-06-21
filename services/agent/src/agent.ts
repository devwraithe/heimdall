import { GoogleGenAI } from "@google/genai";
import type { OperationalSnapshot, RetryDecision } from "./types";
import "dotenv/config";
import dotenv from "dotenv";
import path from "path";
dotenv.config({ path: path.resolve("../../.env") });

const ai = new GoogleGenAI({ apiKey: process.env.GEMINI_API_KEY! });

export async function makeRetryDecision(
  snapshot: OperationalSnapshot,
): Promise<RetryDecision> {
  const failedBundles = snapshot.recentOutcomes.filter(
    (o) => o.stage === "Failed" || o.failureReason !== "",
  );

  if (failedBundles.length === 0) {
    return {
      shouldRetry: false,
      reason: "No failed bundles detected",
      refreshBlockhash: false,
      suggestedTipLamports: snapshot.tipMedianLamports,
      failureClassification: "None",
    };
  }

  const prompt = `
You are an autonomous Solana transaction retry agent for a Jito bundle submission system.

Analyze the following operational snapshot and decide whether to retry failed bundles.

Current network state:
- Current slot: ${snapshot.currentSlot}
- Latest finalized slot: ${snapshot.latestFinalizedSlot}
- Slot gap (processing lag): ${snapshot.currentSlot - snapshot.latestFinalizedSlot}
- Active bundles in flight: ${snapshot.activeBundleCount}
- Current tip median: ${snapshot.tipMedianLamports} lamports

Failed bundles (${failedBundles.length}):
${failedBundles
  .map(
    (b) => `
  - Bundle ID: ${b.bundleId}
  - Target slot: ${b.slot}
  - Failure reason: ${b.failureReason || "Unknown"}
  - Tip paid: ${b.tipLamports} lamports
  - Blockhash: ${b.blockhash}
`,
  )
  .join("")}

Based on this data, reason through the following:
1. What caused the failure?
2. Should we retry?
3. Do we need a fresh blockhash?
4. What tip amount should we use for the retry?

Respond ONLY with a valid JSON object in this exact format:
{
  "shouldRetry": true or false,
  "reason": "your reasoning here",
  "refreshBlockhash": true or false,
  "suggestedTipLamports": <number>,
  "failureClassification": "ExpiredBlockhash" | "FeeTooLow" | "BundleFailure" | "ComputeExceeded" | "Unknown"
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
  console.log("Agent decision:", JSON.stringify(decision, null, 2));
  return decision;
}
