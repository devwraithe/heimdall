import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import path from "path";
import type { OperationalSnapshot } from "./types";

const PROTO_PATH = path.resolve("../../proto/heimdall.proto");
const L5_ADDRESS = "localhost:50051";

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
  });

  stream.on("end", () => {
    console.log("L5 stream ended, reconnecting in 2s...");
    setTimeout(() => createL5Stream(onSnapshot, onError), 2000);
  });
}
