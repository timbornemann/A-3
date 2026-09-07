ALTER TABLE agent_replan_research_checkpoints ADD COLUMN pending_need TEXT
    CHECK (pending_need IS NULL OR length(CAST(pending_need AS BLOB)) BETWEEN 1 AND 65536);
