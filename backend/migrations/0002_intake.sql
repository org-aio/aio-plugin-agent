ALTER TABLE agent_conversations ALTER COLUMN provider_id DROP NOT NULL;
ALTER TABLE agent_conversations ADD COLUMN space_id TEXT;
ALTER TABLE agent_messages ADD COLUMN source_id TEXT;
ALTER TABLE agent_messages ADD COLUMN memory_status TEXT;
ALTER TABLE agent_messages ADD COLUMN citations JSONB NOT NULL DEFAULT '[]';
ALTER TABLE agent_messages DROP CONSTRAINT agent_messages_status_check;
ALTER TABLE agent_messages ADD CONSTRAINT agent_messages_status_check CHECK(status IN ('queued','complete','generating','cancelled','failed','interrupted'));
CREATE TABLE agent_intake (
    message_id UUID PRIMARY KEY REFERENCES agent_messages(id) ON DELETE CASCADE,
    conversation_id UUID NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
    request_id UUID NOT NULL,
    ciphertext BYTEA,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','captured','complete','failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(conversation_id,request_id)
);
CREATE INDEX agent_intake_queue ON agent_intake(state,available_at);
