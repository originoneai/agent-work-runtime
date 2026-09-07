-- A revision-bound derived cache. Only curated, bounded and redacted summaries enter FTS.
CREATE TABLE search_documents (
 rowid INTEGER PRIMARY KEY,
 project_id TEXT NOT NULL REFERENCES projects(id),
 entity_id TEXT NOT NULL, kind TEXT NOT NULL,
 external_key TEXT NOT NULL, title TEXT NOT NULL, summary TEXT NOT NULL, terms TEXT NOT NULL,
 status TEXT, work_item_key TEXT,
 source_id TEXT, source_ref_json TEXT CHECK(source_ref_json IS NULL OR json_valid(source_ref_json)),
 revision INTEGER NOT NULL,
 UNIQUE(project_id,kind,entity_id),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id)
) STRICT;
CREATE INDEX search_filter ON search_documents(project_id,kind,status,work_item_key);
CREATE VIRTUAL TABLE search_fts USING fts5(external_key,title,summary,terms,
 content='search_documents',content_rowid='rowid',tokenize='unicode61');
CREATE TABLE search_state (
 project_id TEXT PRIMARY KEY REFERENCES projects(id),
 project_revision INTEGER NOT NULL,
 policy_version INTEGER NOT NULL
) STRICT;
