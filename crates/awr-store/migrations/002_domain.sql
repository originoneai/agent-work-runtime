-- Source projections retain provenance; runtime objects have their own authority.
CREATE TABLE projects (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 external_key TEXT NOT NULL UNIQUE,
 name TEXT NOT NULL,
 root TEXT NOT NULL UNIQUE,
 authority_mode TEXT NOT NULL CHECK(authority_mode='source_first'),
 current_branch_id TEXT,
 project_revision INTEGER NOT NULL DEFAULT 0 CHECK(project_revision>=0),
 FOREIGN KEY(id,current_branch_id) REFERENCES branches(project_id,id)
) STRICT;

CREATE TABLE sources (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 domain TEXT NOT NULL,
 role TEXT NOT NULL CHECK(role IN ('primary','supporting')),
 locator TEXT NOT NULL,
 format TEXT NOT NULL,
 adapter TEXT NOT NULL,
 revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0),
 fingerprint TEXT NOT NULL DEFAULT '',
 freshness TEXT NOT NULL CHECK(freshness IN ('fresh','stale','unavailable')),
 active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
 config_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(config_json)),
 UNIQUE(project_id,domain,locator),
 UNIQUE(project_id,id)
) STRICT;

CREATE TABLE goals (
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
 status TEXT NOT NULL,
 priority TEXT,
 success_criteria_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(success_criteria_json)),
 UNIQUE(project_id,external_key),
 UNIQUE(project_id,id),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id)
) STRICT;
CREATE INDEX goals_source ON goals(project_id,source_id,active);
CREATE TABLE plans (
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
 status TEXT NOT NULL,
 summary TEXT NOT NULL DEFAULT '',
 scope_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(scope_json)),
 UNIQUE(project_id,external_key),
 UNIQUE(project_id,id),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id)
) STRICT;
CREATE INDEX plans_source ON plans(project_id,source_id,active);
CREATE TABLE rules (
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
 text TEXT NOT NULL,
 severity TEXT CHECK(severity IN ('hard','soft','info')),
 scope_json TEXT CHECK(scope_json IS NULL OR json_valid(scope_json)),
 unresolved_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(unresolved_json)),
 UNIQUE(project_id,external_key),
 UNIQUE(project_id,id),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id)
) STRICT;
CREATE INDEX rules_source ON rules(project_id,source_id,active);
CREATE TABLE work_items (
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
 status TEXT NOT NULL CHECK(status IN ('planned','ready','claimed','in_progress','blocked','completed','cancelled','unknown')),
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
CREATE INDEX work_items_source ON work_items(project_id,source_id,active);
CREATE TABLE decisions (
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
 status TEXT NOT NULL CHECK(status IN ('proposed','accepted','superseded','rejected','unknown')),
 decision TEXT NOT NULL,
 rationale TEXT NOT NULL DEFAULT '',
 affected_keys_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(affected_keys_json)),
 paths_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(paths_json)),
 UNIQUE(project_id,external_key),
 UNIQUE(project_id,id),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id)
) STRICT;
CREATE INDEX decisions_source ON decisions(project_id,source_id,active);

CREATE INDEX work_status ON work_items(project_id,status,active,priority);

CREATE TABLE edges (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 from_kind TEXT NOT NULL, from_key TEXT NOT NULL,
 relation TEXT NOT NULL, to_kind TEXT NOT NULL, to_key TEXT NOT NULL,
 required INTEGER NOT NULL DEFAULT 1 CHECK(required IN (0,1)),
 source_id TEXT NOT NULL,
 source_ref_json TEXT NOT NULL CHECK(json_valid(source_ref_json)),
 source_revision INTEGER NOT NULL CHECK(source_revision>=0),
 revision INTEGER NOT NULL CHECK(revision>0),
 active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
 UNIQUE(project_id,source_id,from_kind,from_key,relation,to_kind,to_key),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id)
) STRICT;
CREATE INDEX edge_from ON edges(project_id,from_kind,from_key,relation,active);

CREATE TABLE branches (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 name TEXT NOT NULL,
 parent_branch_id TEXT,
 git_ref TEXT,
 fork_project_revision INTEGER NOT NULL CHECK(fork_project_revision>=0),
 status TEXT NOT NULL CHECK(status IN ('active','merged','abandoned')),
 revision INTEGER NOT NULL CHECK(revision>0),
 UNIQUE(project_id,name), UNIQUE(project_id,id),
 FOREIGN KEY(project_id,parent_branch_id) REFERENCES branches(project_id,id)
) STRICT;

CREATE TABLE sessions (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 work_item_id TEXT, branch_id TEXT,
 agent_id TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('active','ended','interrupted','incomplete')),
 started_at INTEGER NOT NULL, ended_at INTEGER,
 start_project_revision INTEGER NOT NULL CHECK(start_project_revision>=0),
 end_project_revision INTEGER CHECK(end_project_revision>=0),
 last_checkpoint_id TEXT,
 revision INTEGER NOT NULL CHECK(revision>0),
 UNIQUE(project_id,id),
 FOREIGN KEY(project_id,work_item_id) REFERENCES work_items(project_id,id),
 FOREIGN KEY(project_id,branch_id) REFERENCES branches(project_id,id),
 FOREIGN KEY(last_checkpoint_id) REFERENCES checkpoints(id)
) STRICT;

CREATE TABLE claims (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 work_item_id TEXT NOT NULL, session_id TEXT NOT NULL, agent_id TEXT NOT NULL, branch_id TEXT,
 status TEXT NOT NULL CHECK(status IN ('active','released','expired')),
 acquired_at INTEGER NOT NULL, expires_at INTEGER, released_at INTEGER,
 revision INTEGER NOT NULL CHECK(revision>0),
 FOREIGN KEY(project_id,work_item_id) REFERENCES work_items(project_id,id),
 FOREIGN KEY(project_id,session_id) REFERENCES sessions(project_id,id),
 FOREIGN KEY(project_id,branch_id) REFERENCES branches(project_id,id)
) STRICT;
CREATE UNIQUE INDEX active_claim ON claims(project_id,work_item_id,coalesce(branch_id,'')) WHERE status='active';

CREATE TABLE events (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 work_item_id TEXT, session_id TEXT, branch_id TEXT,
 event_type TEXT NOT NULL, importance TEXT NOT NULL,
 summary TEXT NOT NULL DEFAULT '',
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
 project_revision INTEGER NOT NULL CHECK(project_revision>0),
 created_at INTEGER NOT NULL,
 UNIQUE(project_id,id),
 FOREIGN KEY(project_id,work_item_id) REFERENCES work_items(project_id,id),
 FOREIGN KEY(project_id,session_id) REFERENCES sessions(project_id,id),
 FOREIGN KEY(project_id,branch_id) REFERENCES branches(project_id,id)
) STRICT;
CREATE INDEX event_delta ON events(project_id,project_revision,created_at,id);
CREATE TRIGGER event_no_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT,'events are append-only'); END;
CREATE TRIGGER event_no_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT,'events are append-only'); END;

CREATE TABLE checkpoints (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 session_id TEXT NOT NULL REFERENCES sessions(id),
 project_revision INTEGER NOT NULL CHECK(project_revision>=0),
 context_hash TEXT NOT NULL, digest TEXT NOT NULL, next_action TEXT NOT NULL,
 open_loops_json TEXT NOT NULL CHECK(json_valid(open_loops_json)),
 changed_entities_json TEXT NOT NULL CHECK(json_valid(changed_entities_json)),
 revision INTEGER NOT NULL CHECK(revision>0), created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE artifacts (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 artifact_type TEXT NOT NULL, locator TEXT NOT NULL,
 sha256 TEXT NOT NULL, size INTEGER NOT NULL CHECK(size>=0), mime TEXT NOT NULL,
 source_event_id TEXT, revision INTEGER NOT NULL CHECK(revision>0),
 UNIQUE(project_id,id),
 FOREIGN KEY(project_id,source_event_id) REFERENCES events(project_id,id)
) STRICT;

CREATE TABLE evidence (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 work_item_id TEXT, external_key TEXT NOT NULL,
 evidence_type TEXT NOT NULL,
 level TEXT NOT NULL CHECK(level IN ('designed','implemented','locally_verified','real_environment_validated','release_candidate','released','unknown')),
 summary TEXT NOT NULL, locator TEXT NOT NULL,
 sha256 TEXT, source_sha TEXT, command TEXT,
 scope_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(scope_json)),
 source_id TEXT, source_revision INTEGER, source_ref_json TEXT CHECK(source_ref_json IS NULL OR json_valid(source_ref_json)),
 branch_id TEXT, revision INTEGER NOT NULL CHECK(revision>0), verified_at INTEGER,
 active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
 UNIQUE(project_id,external_key), UNIQUE(project_id,id),
 CHECK((source_id IS NULL AND source_revision IS NULL AND source_ref_json IS NULL) OR
       (source_id IS NOT NULL AND source_revision IS NOT NULL AND source_ref_json IS NOT NULL)),
 FOREIGN KEY(project_id,work_item_id) REFERENCES work_items(project_id,id),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id),
 FOREIGN KEY(project_id,branch_id) REFERENCES branches(project_id,id)
) STRICT;

CREATE TABLE mutation_proposals (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 work_item_id TEXT, source_id TEXT NOT NULL,
 base_fingerprint TEXT NOT NULL, expected_revision INTEGER NOT NULL CHECK(expected_revision>=0),
 mutation_type TEXT NOT NULL, patch_json TEXT NOT NULL CHECK(json_valid(patch_json)),
 status TEXT NOT NULL CHECK(status IN ('draft','ready','approved','applied','conflict','rejected','failed')),
 created_by_session TEXT, revision INTEGER NOT NULL CHECK(revision>0),
 created_at INTEGER NOT NULL, applied_at INTEGER,
 FOREIGN KEY(project_id,work_item_id) REFERENCES work_items(project_id,id),
 FOREIGN KEY(project_id,source_id) REFERENCES sources(project_id,id),
 FOREIGN KEY(project_id,created_by_session) REFERENCES sessions(project_id,id)
) STRICT;

CREATE TABLE context_packs (
 id TEXT PRIMARY KEY CHECK(length(id)=26),
 project_id TEXT NOT NULL REFERENCES projects(id),
 work_item_id TEXT, branch_id TEXT, session_id TEXT, agent_id TEXT,
 project_revision INTEGER NOT NULL CHECK(project_revision>=0),
 source_revisions_json TEXT NOT NULL CHECK(json_valid(source_revisions_json)),
 token_budget INTEGER NOT NULL CHECK(token_budget>0),
 token_estimate INTEGER NOT NULL CHECK(token_estimate>=0),
 completeness_json TEXT NOT NULL CHECK(json_valid(completeness_json)),
 rendered_context TEXT NOT NULL, context_hash TEXT NOT NULL,
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
 revision INTEGER NOT NULL CHECK(revision>0), created_at INTEGER NOT NULL,
 FOREIGN KEY(project_id,work_item_id) REFERENCES work_items(project_id,id),
 FOREIGN KEY(project_id,branch_id) REFERENCES branches(project_id,id),
 FOREIGN KEY(project_id,session_id) REFERENCES sessions(project_id,id)
) STRICT;
CREATE INDEX context_hash ON context_packs(project_id,context_hash);
