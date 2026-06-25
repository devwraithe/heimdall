FROM oven/bun:1-alpine

WORKDIR /app

COPY services/agent/package.json services/agent/bun.lock* ./
RUN bun install --frozen-lockfile || bun install

COPY services/agent/src/ ./src/
COPY services/agent/tsconfig.json ./
COPY proto/ /app/proto/

# Bun resolves the proto relative to the working directory
ENV PROTO_PATH=/app/proto/heimdall.proto

CMD ["bun", "run", "src/index.ts"]
