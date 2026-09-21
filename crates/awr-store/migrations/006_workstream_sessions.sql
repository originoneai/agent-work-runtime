CREATE TABLE session_workstreams (
 project_id TEXT NOT NULL,
 session_id TEXT NOT NULL,
 work_item_id TEXT,
 workstream_id TEXT,
 ownership_revision INTEGER CHECK(ownership_revision>0),
 authority_version INTEGER CHECK(authority_version>0),
 PRIMARY KEY(project_id,session_id),
 CHECK((workstream_id IS NULL AND authority_version IS NULL) OR
       (workstream_id IS NOT NULL AND authority_version IS NOT NULL)),
 CHECK((work_item_id IS NULL AND ownership_revision IS NULL) OR
       (work_item_id IS NOT NULL AND ownership_revision IS NOT NULL AND workstream_id IS NOT NULL)),
 FOREIGN KEY(project_id,session_id) REFERENCES sessions(project_id,id),
 FOREIGN KEY(project_id,work_item_id) REFERENCES work_items(project_id,id),
 FOREIGN KEY(project_id,workstream_id) REFERENCES workstreams(project_id,id)
) STRICT;
CREATE INDEX session_workstream_scope ON session_workstreams(project_id,workstream_id,session_id);
CREATE TRIGGER session_workstream_no_update BEFORE UPDATE ON session_workstreams BEGIN SELECT RAISE(ABORT,'session scope is immutable'); END;
CREATE TRIGGER session_workstream_no_delete BEFORE DELETE ON session_workstreams BEGIN SELECT RAISE(ABORT,'session scope is retained'); END;
CREATE TRIGGER session_identity_no_update BEFORE UPDATE OF project_id,work_item_id,branch_id ON sessions
 WHEN NEW.project_id IS NOT OLD.project_id OR NEW.work_item_id IS NOT OLD.work_item_id OR NEW.branch_id IS NOT OLD.branch_id
 BEGIN SELECT RAISE(ABORT,'session identity is immutable'); END;

CREATE TABLE conversation_workstreams (
 project_id TEXT NOT NULL,
 client TEXT NOT NULL,
 conversation TEXT NOT NULL,
 workstream_id TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK(revision>0),
 PRIMARY KEY(project_id,client,conversation),
 FOREIGN KEY(project_id,workstream_id) REFERENCES workstreams(project_id,id)
) STRICT;

-- Keep historical branch attribution, but branches cannot confer extra ownership.
CREATE TRIGGER workstream_claim_exclusive_insert BEFORE INSERT ON claims WHEN NEW.status='active'
 BEGIN SELECT CASE WHEN EXISTS(SELECT 1 FROM claims c WHERE c.project_id=NEW.project_id AND c.work_item_id=NEW.work_item_id
   AND c.status='active' AND c.released_at IS NULL AND (c.expires_at IS NULL OR c.expires_at>NEW.acquired_at))
   THEN RAISE(ABORT,'work is already claimed across branches') END; END;
CREATE TRIGGER workstream_claim_exclusive_update BEFORE UPDATE ON claims WHEN NEW.status='active' AND NEW.released_at IS NULL
 AND (NEW.expires_at IS NULL OR NEW.expires_at>CAST(unixepoch('subsec')*1000 AS INTEGER))
 BEGIN SELECT CASE WHEN EXISTS(SELECT 1 FROM claims c WHERE c.project_id=NEW.project_id AND c.work_item_id=NEW.work_item_id
   AND c.id!=NEW.id AND c.status='active' AND c.released_at IS NULL
   AND (c.expires_at IS NULL OR c.expires_at>CAST(unixepoch('subsec')*1000 AS INTEGER)))
   THEN RAISE(ABORT,'work is already claimed across branches') END; END;
