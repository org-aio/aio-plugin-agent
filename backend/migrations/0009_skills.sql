CREATE TABLE agent_skill_files (
    tenant_id TEXT NOT NULL, user_id TEXT NOT NULL, path TEXT NOT NULL,
    hash TEXT, content BYTEA, executable BOOLEAN NOT NULL DEFAULT FALSE,
    size BIGINT NOT NULL DEFAULT 0, updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id,user_id,path)
);
CREATE TABLE agent_skill_history (
    id BIGSERIAL PRIMARY KEY, tenant_id TEXT NOT NULL, user_id TEXT NOT NULL,
    path TEXT NOT NULL, hash TEXT, content BYTEA, executable BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE agent_skill_devices (
    tenant_id TEXT NOT NULL, user_id TEXT NOT NULL, device_id TEXT NOT NULL,
    report JSONB NOT NULL, resolutions JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id,user_id,device_id)
);
