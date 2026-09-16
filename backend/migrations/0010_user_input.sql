ALTER TABLE agent_conversations ADD COLUMN worker_id UUID;
ALTER TABLE agent_messages DROP CONSTRAINT agent_messages_status_check;
ALTER TABLE agent_messages ADD CONSTRAINT agent_messages_status_check CHECK(status IN ('queued','complete','generating','awaiting_input','cancelled','failed','interrupted'));
CREATE TABLE agent_user_inputs (
    id UUID PRIMARY KEY,
    conversation_id UUID NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
    assistant_id UUID NOT NULL REFERENCES agent_messages(id) ON DELETE CASCADE,
    ciphertext BYTEA NOT NULL,
    answer_ciphertext BYTEA,
    state TEXT NOT NULL CHECK(state IN ('pending','answered','cancelled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX agent_one_pending_input ON agent_user_inputs(conversation_id) WHERE state='pending';
