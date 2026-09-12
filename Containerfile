FROM node:22.23.1-bookworm-slim@sha256:6c74791e557ce11fc957704f6d4fe134a7bc8d6f5ca4403205b2966bd488f6b3 AS dependencies
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts --no-audit --no-fund

FROM node:22.23.1-bookworm-slim@sha256:6c74791e557ce11fc957704f6d4fe134a7bc8d6f5ca4403205b2966bd488f6b3 AS runtime
WORKDIR /app
COPY --from=dependencies --chown=node:node /app/node_modules ./node_modules
COPY --chown=node:node dist/runtime ./runtime
ENV AIO_AGENT_BIND=0.0.0.0 AIO_PLUGIN_PORT=8080 AIO_AGENT_RUNTIME_DIR=/app/runtime AIO_AGENT_NODE=/usr/local/bin/node
USER node

FROM runtime AS standalone
COPY --chown=node:node dist/agent-server ./agent-server
EXPOSE 8080
ENTRYPOINT ["/app/agent-server"]
