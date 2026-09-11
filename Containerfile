FROM gcr.io/distroless/static-debian12:nonroot@sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab
COPY --chown=65532:65532 dist/agent-server /agent-server
ENV AIO_AGENT_BIND=0.0.0.0 AIO_PLUGIN_PORT=8080
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/agent-server"]
