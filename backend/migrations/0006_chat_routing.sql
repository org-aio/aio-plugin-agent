ALTER TABLE agent_messages ADD COLUMN route TEXT;
ALTER TABLE agent_messages ADD COLUMN matched_node_ids JSONB NOT NULL DEFAULT '[]';
ALTER TABLE agent_messages ADD COLUMN activated_node_ids JSONB NOT NULL DEFAULT '[]';
