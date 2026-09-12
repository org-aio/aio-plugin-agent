CREATE TABLE agent_intake_fingerprints (
    message_id UUID PRIMARY KEY REFERENCES agent_messages(id) ON DELETE CASCADE,
    ciphertext BYTEA NOT NULL
);
