CREATE TABLE agent_providers (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    label TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    model TEXT NOT NULL,
    secret BYTEA,
    UNIQUE (tenant_id, user_id, id)
);
CREATE TABLE agent_conversations (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    title TEXT NOT NULL,
    provider_id UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, user_id, id),
    FOREIGN KEY (tenant_id, user_id, provider_id) REFERENCES agent_providers(tenant_id, user_id, id)
);
CREATE TABLE agent_messages (
    id UUID PRIMARY KEY,
    conversation_id UUID NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
    request_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('complete', 'generating', 'cancelled', 'failed', 'interrupted')),
    error TEXT,
    tokens BIGINT,
    sequence BIGSERIAL NOT NULL,
    UNIQUE (conversation_id, request_id, role)
);
CREATE UNIQUE INDEX agent_one_generation ON agent_messages(conversation_id) WHERE status = 'generating';
CREATE INDEX agent_conversation_owner ON agent_conversations(tenant_id, user_id, updated_at DESC);
