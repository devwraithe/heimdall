import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import path from "path";
import type { OperationalSnapshot } from "./types";
import { updateSnapshot } from "./store";

const PROTO_PATH = path.resolve("../../proto/heimdall.proto");
const L5_ADDRESS = process.env.L5_ADDRESS || "localhost:50051";

export function connectToL5(): void {
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
    console.log("L7 reconnecting to L5 in 2s...");
    setTimeout(connectToL5, 2000);
  };

  const stream = client.Subscribe({});

  stream.on("data", (snapshot: OperationalSnapshot) => {
    updateSnapshot(snapshot);
  });

  stream.on("error", (err: Error) => {
    console.error("L5 stream error:", err.message);
    reconnect();
  });

  stream.on("end", () => {
    reconnect();
  });

  console.log("L7 connected to L5 state stream");
}
