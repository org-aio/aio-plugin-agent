CREATE TABLE agent_tool_settings (
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    tool TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    secret BYTEA,
    PRIMARY KEY (tenant_id, user_id, tool)
);
