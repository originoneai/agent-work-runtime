-- These projections preserve existing work, session, evidence and event IDs.
-- Schema migration supplies a single legacy scope; isolation is enabled separately.
CREATE TABLE workstream_catalogs (
 project_id TEXT PRIMARY KEY REFERENCES projects(id),
 version INTEGER NOT NULL CHECK(version=1),
 mode TEXT NOT NULL CHECK(mode IN ('legacy','source')),
 legacy_default TEXT,
 source_id TEXT,
 source_ref_json TEXT CHECK(source_ref_json IS NULL OR json_valid(source_ref_json)),
 revision INTEGER NOT NULL CHECK(revision>0),
 CHECK((mode='legacy' AND source_id IS NULL AND source_ref_json IS NULL)
    OR (mode='source' AND source_id IS NOT NULL AND source_ref_json IS NOT NULL)),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id)
) STRICT;

CREATE TABLE workstreams (
 project_id TEXT NOT NULL REFERENCES workstream_catalogs(project_id),
 id TEXT NOT NULL CHECK(length(id)=26),
 external_key TEXT NOT NULL,
 active INTEGER NOT NULL CHECK(active IN (0,1)),
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
 PRIMARY KEY(project_id,id),
 UNIQUE(project_id,external_key)
) STRICT;

CREATE TABLE workstream_ownership (
 project_id TEXT NOT NULL,
 work_item_id TEXT NOT NULL,
 workstream_id TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision>0),
 PRIMARY KEY(project_id,work_item_id),
 FOREIGN KEY(project_id,work_item_id) REFERENCES work_items(project_id,id),
 FOREIGN KEY(project_id,workstream_id) REFERENCES workstreams(project_id,id)
) STRICT;
CREATE INDEX workstream_members ON workstream_ownership(project_id,workstream_id,work_item_id);
