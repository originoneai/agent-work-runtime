CREATE TABLE source_content_reviews (
    source_id TEXT PRIMARY KEY REFERENCES sources(id),
    project_id TEXT NOT NULL REFERENCES projects(id),
    source_fingerprint TEXT NOT NULL,
    source_config_json TEXT NOT NULL,
    adapter TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    receipt_id TEXT NOT NULL,
    policy_version INTEGER NOT NULL
);
