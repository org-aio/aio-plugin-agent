# 只借用固定 Debian 系统库；正式运行镜像不包含 Node、SDK 或构建工具。
FROM node:22.23.1-bookworm-slim@sha256:6c74791e557ce11fc957704f6d4fe134a7bc8d6f5ca4403205b2966bd488f6b3 AS system
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
FROM scratch AS runtime
COPY --from=system /lib/x86_64-linux-gnu/libc.so.6 /lib/x86_64-linux-gnu/libgcc_s.so.1 /lib/x86_64-linux-gnu/libm.so.6 /lib/x86_64-linux-gnu/libdl.so.2 /lib/x86_64-linux-gnu/libpthread.so.0 /lib/x86_64-linux-gnu/librt.so.1 /lib/x86_64-linux-gnu/libresolv.so.2 /lib/x86_64-linux-gnu/
COPY --from=system /lib/x86_64-linux-gnu/ld-linux-x86-64.so.2 /lib64/ld-linux-x86-64.so.2
COPY --from=system /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
WORKDIR /app
ENV AIO_AGENT_BIND=0.0.0.0 AIO_PLUGIN_PORT=8080
USER 65532:65532

FROM runtime AS standalone
COPY --chown=65532:65532 dist/agent-server ./agent-server
EXPOSE 8080
ENTRYPOINT ["/app/agent-server"]
