import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import path from "path";
import type {
  AgentDecision,
  OperationalSnapshot,
  RetryRequest,
  RetryResponse,
} from "./types";

const PROTO_PATH = path.resolve("../../proto/heimdall.proto");
const L5_ADDRESS = process.env.L5_ADDRESS || "localhost:50051";
const L7_ADDRESS = process.env.L7_ADDRESS || "http://localhost:3000";

export function createL5Stream(
  onSnapshot: (snapshot: OperationalSnapshot) => void,
  onError: (error: Error) => void,
): void {
  const packageDef = protoLoader.loadSync(PROTO_PATH, {
    keepCase: false,
    longs: Number,
    enums: String,
    defaults: true,
    oneofs: true,
  });

  const proto = grpc.loadPackageDefinition(packageDef) as any;
  const client = new proto.heimdall.OperationalStateService(
    L5_ADDRESS,
    grpc.credentials.createInsecure(),
  );

  let reconnecting = false;
  const reconnect = () => {
    if (reconnecting) return;
    reconnecting = true;
    console.log("L5 stream reconnecting in 2s...");
    setTimeout(() => createL5Stream(onSnapshot, onError), 2000);
  };

  const stream = client.Subscribe({});
  console.log("Stream created, waiting for data...");

  stream.on("status", (status: any) => {
    console.log("Stream status:", status);
  });

  console.log("Loading proto from:", PROTO_PATH);

  stream.on("data", (snapshot: OperationalSnapshot) => {
    onSnapshot(snapshot);
  });

  stream.on("error", (err: Error) => {
    onError(err);
    reconnect();
  });

  stream.on("end", () => {
    reconnect();
  });
}

export function sendRetryDecision(
  request: RetryRequest,
): Promise<RetryResponse> {
  return new Promise((resolve, reject) => {
    const packageDef = protoLoader.loadSync(PROTO_PATH, {
      keepCase: false,
      longs: Number,
      enums: String,
      defaults: true,
      oneofs: true,
    });

    const proto = grpc.loadPackageDefinition(packageDef) as any;
    const client = new proto.heimdall.OperationalStateService(
      L5_ADDRESS,
      grpc.credentials.createInsecure(),
    );

    client.Retry(request, (err: Error | null, response: RetryResponse) => {
      if (err) reject(err);
      else resolve(response);
    });
  });
}

export async function recordDecision(decision: AgentDecision): Promise<void> {
  try {
    const response = await fetch(`${L7_ADDRESS}/decisions`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(decision),
    });

    if (!response.ok) {
      console.warn("Failed to record agent decision:", response.status);
    }
  } catch (error) {
    console.warn("Failed to record agent decision:", error);
  }
}
