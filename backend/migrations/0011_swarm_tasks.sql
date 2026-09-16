CREATE TABLE agent_swarm_tasks (
    id UUID PRIMARY KEY,
    conversation_id UUID NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
    assistant_id UUID NOT NULL REFERENCES agent_messages(id) ON DELETE CASCADE,
    worker_id UUID NOT NULL,
    device TEXT NOT NULL,
    label TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'unconfirmed',
    ciphertext BYTEA,
    cancelled BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX agent_swarm_conversation ON agent_swarm_tasks(conversation_id,created_at);
