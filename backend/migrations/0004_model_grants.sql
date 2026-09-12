CREATE TABLE agent_model_grants (
    provider_id UUID NOT NULL REFERENCES agent_providers(id) ON DELETE CASCADE,
    space_id TEXT NOT NULL,
    PRIMARY KEY(provider_id,space_id)
);
