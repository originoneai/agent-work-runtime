-- Rebuild only the work status constraint; IDs and all existing rows are retained.
CREATE TABLE work_items_v4 (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 external_key TEXT NOT NULL,
 title TEXT NOT NULL DEFAULT '',
 source_id TEXT NOT NULL,
 source_ref_json TEXT NOT NULL CHECK(json_valid(source_ref_json)),
 source_revision INTEGER NOT NULL CHECK(source_revision>=0),
 revision INTEGER NOT NULL CHECK(revision>0),
 active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
 updated_at INTEGER NOT NULL,
 kind TEXT,
 required INTEGER NOT NULL DEFAULT 0 CHECK(required IN (0,1)),
 raw_status TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('draft','planned','ready','claimed','in_progress','blocked','completed','cancelled','unknown')),
 priority TEXT,
 milestone TEXT,
 score INTEGER,
 evidence_level TEXT,
 summary TEXT NOT NULL DEFAULT '',
 next_action TEXT NOT NULL DEFAULT '',
 blocker TEXT,
 acceptance_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(acceptance_json)),
 tags_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(tags_json)),
 paths_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(paths_json)),
 UNIQUE(project_id,external_key),
 UNIQUE(project_id,id),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id)
) STRICT;
INSERT INTO work_items_v4 SELECT * FROM work_items;
DROP TABLE work_items;
ALTER TABLE work_items_v4 RENAME TO work_items;
CREATE INDEX work_items_source ON work_items(project_id,source_id,active);
CREATE INDEX work_status ON work_items(project_id,status,active,priority);
