CREATE TABLE agent_message_archive (
    message_id UUID PRIMARY KEY REFERENCES agent_messages(id) ON DELETE CASCADE,
    ciphertext BYTEA NOT NULL
);
