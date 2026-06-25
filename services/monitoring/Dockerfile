FROM oven/bun:1-alpine

WORKDIR /app

COPY services/monitoring/package.json services/monitoring/bun.lock* ./
RUN bun install --frozen-lockfile || bun install

COPY services/monitoring/src/ ./src/
COPY services/monitoring/tsconfig.json ./
COPY proto/ /app/proto/

EXPOSE 3000

CMD ["bun", "run", "src/index.ts"]
