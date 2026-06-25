#!/usr/bin/env bun
/**
 * Heimdall CLI — operational command interface
 *
 * Commands:
 *   monitor   Live TUI dashboard (connects to L7 SSE)
 *   status    Print current system status
 *   evidence  Download evidence report
 *   health    Check all service health
 *   start     Launch all services
 */

import dotenv from "dotenv";
import path from "path";
dotenv.config({ path: path.resolve("../../.env") });

const MONITOR_URL = process.env.MONITOR_URL || "http://localhost:3000";

// ─── ANSI helpers ────────────────────────────────────────
const ESC = "\x1b";
const CLEAR = `${ESC}[2J${ESC}[H`;
const HIDE_CURSOR = `${ESC}[?25l`;
const SHOW_CURSOR = `${ESC}[?25h`;
const BOLD = `${ESC}[1m`;
const DIM = `${ESC}[2m`;
const RESET = `${ESC}[0m`;
const RED = `${ESC}[31m`;
const GREEN = `${ESC}[32m`;
const YELLOW = `${ESC}[33m`;
const BLUE = `${ESC}[34m`;
const MAGENTA = `${ESC}[35m`;
const CYAN = `${ESC}[36m`;
const WHITE = `${ESC}[37m`;
const BG_DARK = `${ESC}[48;5;235m`;
const BG_HEADER = `${ESC}[48;5;24m`;

function pad(str: string, len: number): string {
  return str.slice(0, len).padEnd(len);
}

function rightPad(str: string, len: number): string {
  return str.slice(0, len).padStart(len);
}

function statusColor(stage: string): string {
  if (stage === "Finalized") return GREEN;
  if (stage === "Confirmed") return CYAN;
  if (stage === "Processed") return YELLOW;
  if (stage === "Failed") return RED;
  return DIM;
}

function sourceColor(source: string): string {
  if (source === "gemini") return MAGENTA;
  if (source === "local_rules") return CYAN;
  if (source === "fallback") return YELLOW;
  return DIM;
}

function confidenceBar(confidence: number): string {
  const filled = Math.round(confidence * 10);
  const empty = 10 - filled;
  const color = confidence >= 0.8 ? GREEN : confidence >= 0.5 ? YELLOW : RED;
  return `${color}${"█".repeat(filled)}${DIM}${"░".repeat(empty)}${RESET}`;
}

// ─── TUI Monitor ─────────────────────────────────────────

interface SSEData {
  currentSlot: number;
  latestFinalizedSlot: number;
  tipMedianLamports: number;
  activeBundleCount: number;
  totalSubmitted: number;
  totalFailed: number;
  totalFinalized: number;
  uptimeSeconds: number;
  recentOutcomes: Array<{
    bundleId: string;
    slot: number;
    stage: string;
    failureReason: string;
    failureStage: string;
    recovery: string;
    tipLamports: number;
    originalBundleId: string;
    retryAttempt: number;
  }>;
  recentDecisions: Array<{
    timestamp: number;
    shouldRetry: boolean;
    reason: string;
    failureClassification: string;
    suggestedTipLamports: number;
    confidence: number;
    observedRisk: string;
    source: string;
  }>;
}

function renderTUI(data: SSEData): string {
  const cols = process.stdout.columns || 120;
  const slotGap = data.currentSlot - data.latestFinalizedSlot;
  const uptime = formatUptime(data.uptimeSeconds);
  const successRate =
    data.totalSubmitted > 0
      ? ((data.totalFinalized / data.totalSubmitted) * 100).toFixed(1)
      : "—";

  let out = CLEAR;

  // ─── Header ───
  out += `${BG_HEADER}${BOLD}${WHITE}`;
  out += ` ⚡ HEIMDALL — Smart Transaction Stack`.padEnd(cols);
  out += `${RESET}\n`;
  out += `${BG_HEADER}${DIM}${WHITE}`;
  out += ` Real-time Solana Infrastructure Monitor`.padEnd(cols);
  out += `${RESET}\n\n`;

  // ─── Network Status ───
  out += `${BOLD}${CYAN} ◉ NETWORK STATUS${RESET}\n`;
  out += `${DIM}${"─".repeat(cols)}${RESET}\n`;

  const slotGapColor = slotGap > 50 ? RED : slotGap > 30 ? YELLOW : GREEN;
  out += `  ${BOLD}Current Slot${RESET}    ${WHITE}${data.currentSlot.toLocaleString()}${RESET}`;
  out += `    ${BOLD}Finalized${RESET}  ${WHITE}${data.latestFinalizedSlot.toLocaleString()}${RESET}`;
  out += `    ${BOLD}Gap${RESET}  ${slotGapColor}${slotGap}${RESET}`;
  out += `    ${BOLD}Tip${RESET}  ${WHITE}${data.tipMedianLamports.toLocaleString()} lam${RESET}`;
  out += `\n`;
  out += `  ${BOLD}Uptime${RESET}          ${DIM}${uptime}${RESET}`;
  out += `    ${BOLD}Active${RESET}     ${WHITE}${data.activeBundleCount}${RESET}`;
  out += `    ${BOLD}Rate${RESET} ${Number(successRate) >= 80 ? GREEN : YELLOW}${successRate}%${RESET}`;
  out += `\n\n`;

  // ─── Bundle Stats ───
  out += `${BOLD}${CYAN} ◉ BUNDLE METRICS${RESET}\n`;
  out += `${DIM}${"─".repeat(cols)}${RESET}\n`;
  out += `  ${GREEN}✓ Finalized ${data.totalFinalized}${RESET}`;
  out += `    ${RED}✗ Failed ${data.totalFailed}${RESET}`;
  out += `    ${WHITE}⊕ Total ${data.totalSubmitted}${RESET}\n\n`;

  // ─── Bundle Outcomes Table ───
  out += `${BOLD}${CYAN} ◉ RECENT BUNDLES${RESET}\n`;
  out += `${DIM}${"─".repeat(cols)}${RESET}\n`;
  out += `${BG_DARK}${BOLD}`;
  out += `  ${pad("Bundle ID", 14)} ${pad("Slot", 12)} ${pad("Stage", 12)} ${pad("Failure", 18)} ${pad("Stage", 14)} ${pad("Tip", 14)} ${pad("Retry", 6)}`;
  out += `${RESET}\n`;

  if (data.recentOutcomes.length === 0) {
    out += `  ${DIM}No bundles tracked yet...${RESET}\n`;
  } else {
    for (const o of data.recentOutcomes.slice(0, 8)) {
      const color = statusColor(o.stage);
      const retryStr =
        o.retryAttempt > 0
          ? `${YELLOW}#${o.retryAttempt}${RESET}`
          : `${DIM}—${RESET}`;
      out += `  ${WHITE}${pad(o.bundleId.slice(0, 12) + "..", 14)}${RESET} `;
      out += `${DIM}${pad(o.slot ? o.slot.toLocaleString() : "—", 12)}${RESET} `;
      out += `${color}${pad(o.stage, 12)}${RESET} `;
      out += `${o.failureReason ? RED : DIM}${pad(o.failureReason || "—", 18)}${RESET} `;
      out += `${o.failureStage ? YELLOW : DIM}${pad(o.failureStage || "—", 14)}${RESET} `;
      out += `${WHITE}${pad(o.tipLamports ? o.tipLamports.toLocaleString() : "—", 14)}${RESET} `;
      out += retryStr;
      out += `\n`;
    }
  }
  out += `\n`;

  // ─── AI Agent Decisions ───
  out += `${BOLD}${CYAN} ◉ AI AGENT DECISIONS${RESET}\n`;
  out += `${DIM}${"─".repeat(cols)}${RESET}\n`;
  out += `${BG_DARK}${BOLD}`;
  out += `  ${pad("Time", 10)} ${pad("Action", 8)} ${pad("Classification", 20)} ${pad("Tip", 14)} ${pad("Confidence", 14)} ${pad("Source", 12)} ${pad("Risk", 30)}`;
  out += `${RESET}\n`;

  if (!data.recentDecisions || data.recentDecisions.length === 0) {
    out += `  ${DIM}No decisions yet...${RESET}\n`;
  } else {
    for (const d of data.recentDecisions.slice(0, 5)) {
      const time = new Date(d.timestamp).toLocaleTimeString("en-GB", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      });
      const action = d.shouldRetry
        ? `${GREEN}RETRY${RESET}`
        : `${YELLOW}HOLD${RESET}`;
      const conf = confidenceBar(d.confidence);
      const src = `${sourceColor(d.source)}${d.source}${RESET}`;
      const risk = d.observedRisk ? d.observedRisk.slice(0, 28) + ".." : "—";
      out += `  ${DIM}${pad(time, 10)}${RESET} `;
      out += `${pad("", 8).replace(/ /g, "")}${action}${" ".repeat(Math.max(0, 8 - (d.shouldRetry ? 5 : 4)))} `;
      out += `${WHITE}${pad(d.failureClassification || "—", 20)}${RESET} `;
      out += `${WHITE}${pad(d.suggestedTipLamports?.toLocaleString() || "—", 14)}${RESET} `;
      out += `${conf} `;
      out += `${src}${" ".repeat(Math.max(0, 12 - d.source.length))} `;
      out += `${DIM}${risk}${RESET}`;
      out += `\n`;
    }
  }
  out += `\n`;

  // ─── Footer ───
  out += `${DIM}${"─".repeat(cols)}${RESET}\n`;
  out += `${DIM}  Press Ctrl+C to exit  │  Refreshing every 1s  │  Source: ${MONITOR_URL}/events${RESET}\n`;

  return out;
}

function formatUptime(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return `${h}h ${m}m ${s}s`;
}

async function cmdMonitor(): Promise<void> {
  process.stdout.write(HIDE_CURSOR);
  process.on("exit", () => process.stdout.write(SHOW_CURSOR));
  process.on("SIGINT", () => {
    process.stdout.write(SHOW_CURSOR);
    process.exit(0);
  });

  console.log(`${CYAN}Connecting to ${MONITOR_URL}/events...${RESET}`);

  try {
    const response = await fetch(`${MONITOR_URL}/events`);
    if (!response.ok || !response.body) {
      console.error(`${RED}Failed to connect: ${response.statusText}${RESET}`);
      process.exit(1);
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() || "";

      for (const line of lines) {
        if (line.startsWith("data: ")) {
          try {
            const data: SSEData = JSON.parse(line.slice(6));
            process.stdout.write(renderTUI(data));
          } catch {
            // Skip malformed SSE data
          }
        }
      }
    }
  } catch (err: any) {
    console.error(
      `${RED}Connection failed: ${err.message}${RESET}\n${DIM}Make sure L7 monitoring is running at ${MONITOR_URL}${RESET}`,
    );
    process.exit(1);
  }
}

// ─── Status ──────────────────────────────────────────────

async function cmdStatus(): Promise<void> {
  try {
    const [healthRes, metricsRes] = await Promise.all([
      fetch(`${MONITOR_URL}/health`),
      fetch(`${MONITOR_URL}/metrics`),
    ]);

    const health = await healthRes.json();
    const metrics = await metricsRes.json();

    console.log(`${BOLD}${CYAN}⚡ Heimdall Status${RESET}\n`);
    console.log(`${BOLD}Health${RESET}`);
    console.log(
      `  Status:     ${health.status === "ok" ? GREEN + "● healthy" : RED + "● unhealthy"}${RESET}`,
    );
    console.log(`  Uptime:     ${formatUptime(health.uptime_seconds)}`);
    console.log(`  Started:    ${health.started_at}\n`);

    console.log(`${BOLD}Network${RESET}`);
    console.log(`  Slot:       ${metrics.current_slot}`);
    console.log(`  Finalized:  ${metrics.latest_finalized_slot}`);
    console.log(
      `  Slot Gap:   ${metrics.current_slot - metrics.latest_finalized_slot}`,
    );
    console.log(`  Tip Median: ${metrics.tip_median_lamports} lamports\n`);

    console.log(`${BOLD}Bundles${RESET}`);
    console.log(`  Submitted:  ${metrics.total_bundles_submitted}`);
    console.log(
      `  Finalized:  ${GREEN}${metrics.total_bundles_finalized}${RESET}`,
    );
    console.log(`  Failed:     ${RED}${metrics.total_bundles_failed}${RESET}`);
    console.log(`  Active:     ${metrics.active_bundle_count}`);
  } catch (err: any) {
    console.error(
      `${RED}Cannot reach monitoring service: ${err.message}${RESET}`,
    );
    console.error(`${DIM}Make sure L7 is running at ${MONITOR_URL}${RESET}`);
    process.exit(1);
  }
}

// ─── Evidence ────────────────────────────────────────────

async function cmdEvidence(): Promise<void> {
  try {
    const res = await fetch(`${MONITOR_URL}/evidence`);
    const report = await res.text();
    const filename = `heimdall-evidence-${Date.now()}.md`;
    await Bun.write(filename, report);
    console.log(`${GREEN}✓ Evidence report saved to ${filename}${RESET}`);
    console.log(
      `${DIM}  ${report.split("\n").length} lines, ${report.length} bytes${RESET}`,
    );
  } catch (err: any) {
    console.error(`${RED}Cannot download evidence: ${err.message}${RESET}`);
    process.exit(1);
  }
}

// ─── Health ──────────────────────────────────────────────

async function cmdHealth(): Promise<void> {
  const services = [{ name: "L7 Monitoring", url: `${MONITOR_URL}/health` }];

  console.log(`${BOLD}${CYAN}⚡ Heimdall Service Health${RESET}\n`);

  for (const svc of services) {
    try {
      const start = performance.now();
      const res = await fetch(svc.url, { signal: AbortSignal.timeout(3000) });
      const latency = Math.round(performance.now() - start);
      const data = await res.json();
      console.log(
        `  ${GREEN}●${RESET} ${pad(svc.name, 20)} ${GREEN}healthy${RESET}  ${DIM}${latency}ms${RESET}  uptime: ${formatUptime(data.uptime_seconds)}`,
      );
    } catch {
      console.log(
        `  ${RED}●${RESET} ${pad(svc.name, 20)} ${RED}unreachable${RESET}`,
      );
    }
  }

  // Check L5 gRPC via monitoring /metrics
  try {
    const res = await fetch(`${MONITOR_URL}/metrics`, {
      signal: AbortSignal.timeout(3000),
    });
    const data = await res.json();
    if (data.current_slot > 0) {
      console.log(
        `  ${GREEN}●${RESET} ${pad("L5 gRPC State", 20)} ${GREEN}streaming${RESET}  slot: ${data.current_slot}`,
      );
    } else {
      console.log(
        `  ${YELLOW}●${RESET} ${pad("L5 gRPC State", 20)} ${YELLOW}no data${RESET}  ${DIM}waiting for snapshots${RESET}`,
      );
    }
  } catch {
    console.log(
      `  ${RED}●${RESET} ${pad("L5 gRPC State", 20)} ${RED}unreachable${RESET}`,
    );
  }
}

// ─── Start ───────────────────────────────────────────────

async function cmdStart(): Promise<void> {
  console.log(`${BOLD}${CYAN}⚡ Starting Heimdall Stack${RESET}\n`);
  console.log(
    `${DIM}This launches all three services in the background.${RESET}`,
  );
  console.log(`${DIM}Use 'heimdall monitor' for live dashboard.${RESET}\n`);

  const procs = [
    { name: "Core (L1-L5)", cmd: ["cargo", "run"], cwd: "../core" },
    {
      name: "Agent (L6)",
      cmd: ["bun", "run", "src/index.ts"],
      cwd: "../services/agent",
    },
    {
      name: "Monitor (L7)",
      cmd: ["bun", "run", "src/index.ts"],
      cwd: "../services/monitoring",
    },
  ];

  for (const p of procs) {
    console.log(`  ${CYAN}▶${RESET} Starting ${p.name}...`);
    Bun.spawn(p.cmd, {
      cwd: p.cwd,
      stdio: ["ignore", "ignore", "ignore"],
    });
    console.log(`  ${GREEN}✓${RESET} ${p.name} launched`);
  }

  console.log(`\n${GREEN}All services started.${RESET}`);
  console.log(
    `${DIM}Run 'heimdall health' to verify, or 'heimdall monitor' for live view.${RESET}`,
  );
}

// ─── Help ────────────────────────────────────────────────

function printHelp(): void {
  console.log(`
${BOLD}${CYAN}⚡ Heimdall CLI${RESET}
${DIM}Smart Solana Transaction Infrastructure Stack${RESET}

${BOLD}USAGE${RESET}
  bun run cli/heimdall.ts <command>

${BOLD}COMMANDS${RESET}
  ${WHITE}monitor${RESET}    Live TUI dashboard with slot pulse, bundle table, and AI decisions
  ${WHITE}status${RESET}     Print current system status (health + metrics)
  ${WHITE}evidence${RESET}   Download judge-ready evidence report (Markdown)
  ${WHITE}health${RESET}     Check health of all services
  ${WHITE}start${RESET}      Launch all three services (core, agent, monitoring)

${BOLD}ENVIRONMENT${RESET}
  ${DIM}MONITOR_URL${RESET}  L7 monitoring URL (default: http://localhost:3000)

${BOLD}EXAMPLES${RESET}
  ${DIM}bun run cli/heimdall.ts monitor${RESET}     # Live TUI dashboard
  ${DIM}bun run cli/heimdall.ts status${RESET}      # Quick status check
  ${DIM}bun run cli/heimdall.ts evidence${RESET}    # Export evidence report
`);
}

// ─── Main ────────────────────────────────────────────────

const command = process.argv[2];

switch (command) {
  case "monitor":
    await cmdMonitor();
    break;
  case "status":
    await cmdStatus();
    break;
  case "evidence":
    await cmdEvidence();
    break;
  case "health":
    await cmdHealth();
    break;
  case "start":
    await cmdStart();
    break;
  default:
    printHelp();
    break;
}
